mod admin;
mod calculator;
mod error;
mod purchase;

use aptos_rust_sdk::client::builder::AptosClientBuilder;
use aptos_rust_sdk::client::config::AptosNetwork;
use aptos_rust_sdk_types::api_types::address::AccountAddress;
use aptos_rust_sdk_types::api_types::chain_id::ChainId;
use error::{ConsumerError, ConsumerResult};
use quark_core::helpers::dto::PurchaseMessage;
use redis::{AsyncCommands, Client};
use reqwest::Client as ReqClient;
use serde_json;
use std::env;
use std::str::FromStr;
use std::time::Duration;

use crate::calculator::handler::get_price;
use crate::purchase::dto::{Purchase, PurchaseType};
use crate::purchase::handler::purchase_ai;

async fn connect_to_redis_with_retry(redis_url: &str) -> redis::aio::MultiplexedConnection {
    let mut retry_count = 0;
    let max_retries = 10;
    let base_delay = Duration::from_secs(1);

    loop {
        println!(
            "Attempting to connect to Redis (attempt {})...",
            retry_count + 1
        );

        match Client::open(redis_url) {
            Ok(client) => match client.get_multiplexed_async_connection().await {
                Ok(connection) => {
                    println!(
                        "Successfully connected to Redis after {} attempts",
                        retry_count + 1
                    );
                    return connection;
                }
                Err(e) => {
                    eprintln!(
                        "Failed to get Redis connection (attempt {}): {}",
                        retry_count + 1,
                        e
                    );
                }
            },
            Err(e) => {
                eprintln!(
                    "Failed to create Redis client (attempt {}): {}",
                    retry_count + 1,
                    e
                );
            }
        }

        retry_count += 1;
        if retry_count >= max_retries {
            panic!("Failed to connect to Redis after {} attempts", max_retries);
        }

        let delay = base_delay * 2_u32.pow(retry_count as u32 - 1);
        println!("Retrying Redis connection in {:?}...", delay);
        tokio::time::sleep(delay).await;
    }
}

async fn process_message_with_retry(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    message: String,
    contract_address: AccountAddress,
    node: aptos_rust_sdk::client::rest_api::AptosFullnodeClient,
    chain_id: ChainId,
    path: &str,
    panora_url: &str,
    panora_api_key: &str,
) -> ConsumerResult<()> {
    let purchase: PurchaseMessage = serde_json::from_str(&message)
        .map_err(|e| ConsumerError::InvalidMessage(format!("Failed to parse message: {}", e)))?;

    let model_name = purchase.model.to_string();
    let total_tokens = purchase.tokens_used;
    let tool_usage = purchase.tools_used;
    let client = ReqClient::builder()
        .user_agent("quark-consumer/1.0")
        .build()
        .map_err(|e| {
            ConsumerError::InvalidMessage(format!("Failed to create HTTP client: {}", e))
        })?;

    let price = get_price(
        &path,
        &panora_url,
        &panora_api_key,
        &model_name,
        &purchase.currency,
        total_tokens as u64,
        tool_usage,
        &client,
    )
    .await;

    if price.is_err() {
        eprintln!("Error getting price: {:?}", price.err());

        // Try to requeue the message
        let _: () = redis_connection
            .lpush("purchase", message)
            .await
            .map_err(|e| {
                ConsumerError::InvalidMessage(format!("Failed to push message to Redis: {}", e))
            })?;

        return Err(ConsumerError::InvalidMessage(
            "Failed to get price".to_string(),
        ));
    }

    let purchase_type = if purchase.group_id.is_some() {
        PurchaseType::Group(purchase.group_id.unwrap())
    } else {
        PurchaseType::User(purchase.account_address)
    };

    let (amount, token_address) = price.unwrap();

    let purchase_query = Purchase::from((
        purchase_type,
        contract_address,
        purchase.coin_version,
        amount,
        token_address,
        node.clone(),
        chain_id,
    ));

    let transaction_response = purchase_ai(purchase_query).await;

    if transaction_response.is_err() {
        eprintln!("Error purchasing: {:?}", transaction_response.err());

        // Try to requeue the message
        let _: () = redis_connection
            .lpush("purchase", message)
            .await
            .map_err(|e| {
                ConsumerError::InvalidMessage(format!("Failed to push message to Redis: {}", e))
            })?;

        return Err(ConsumerError::InvalidMessage(
            "Failed to purchase".to_string(),
        ));
    }

    let transaction_response = transaction_response.unwrap();

    println!("Purchased successfully: {:?}", transaction_response);

    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> ConsumerResult<()> {
    let network = env::var("APTOS_NETWORK").expect("APTOS_NETWORK environment variable not set");
    let contract_address =
        env::var("CONTRACT_ADDRESS").expect("CONTRACT_ADDRESS environment variable not set");
    let redis_url = env::var("REDIS_URL").map_err(|_| {
        ConsumerError::ConnectionFailed("REDIS_URL environment variable not set".to_string())
    })?;
    let path = "assets/prices.ron".to_string();
    let panora_url =
        env::var("PANORA_URL").unwrap_or_else(|_| "https://api.panora.exchange".to_string());
    let panora_api_key = env::var("PANORA_API_KEY").unwrap_or_else(|_| "".to_string());
    let aptos_api_key = env::var("APTOS_API_KEY").unwrap_or_else(|_| "".to_string());

    let (builder, chain_id) = match network.as_str() {
        "mainnet" => (
            AptosClientBuilder::new(AptosNetwork::mainnet()),
            ChainId::Mainnet,
        ),
        "testnet" => (
            AptosClientBuilder::new(AptosNetwork::testnet()),
            ChainId::Testnet,
        ),
        "devnet" => (
            AptosClientBuilder::new(AptosNetwork::devnet()),
            ChainId::Localnet,
        ),
        _ => (
            AptosClientBuilder::new(AptosNetwork::testnet()),
            ChainId::Testnet,
        ),
    };

    let node = if aptos_api_key.is_empty() {
        builder.build()
    } else {
        builder.api_key(aptos_api_key.as_str()).unwrap().build()
    };

    let contract_address = AccountAddress::from_str(&contract_address)
        .expect("CONTRACT_ADDRESS is not a valid account address");

    println!("Starting Quark Consumer...");
    println!("Connecting to Redis");

    // Initial connection with retry
    let mut redis_connection = connect_to_redis_with_retry(&redis_url).await;

    println!("Connected to Redis successfully");
    println!("Starting consumer loop...");

    let mut consecutive_errors = 0;
    let max_consecutive_errors = 5;

    loop {
        match redis_connection
            .rpop::<_, Option<String>>("purchase", None)
            .await
        {
            Ok(outcome) => {
                consecutive_errors = 0; // Reset error counter on successful operation

                match outcome {
                    Some(message) => {
                        // Process the message with retry logic
                        match process_message_with_retry(
                            &mut redis_connection,
                            message,
                            contract_address,
                            node.clone(),
                            chain_id,
                            &path,
                            &panora_url,
                            &panora_api_key,
                        )
                        .await
                        {
                            Ok(_) => {
                                // Message processed successfully
                            }
                            Err(e) => {
                                eprintln!("Failed to process message: {}", e);
                                tokio::time::sleep(Duration::from_secs(5)).await;
                                // Don't return here, continue the loop
                            }
                        }
                    }
                    None => {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
            Err(e) => {
                consecutive_errors += 1;
                eprintln!(
                    "Redis error: {}. Retrying in 5 seconds... (consecutive errors: {})",
                    e, consecutive_errors
                );

                // If we have too many consecutive errors, try to reconnect
                if consecutive_errors >= max_consecutive_errors {
                    eprintln!("Too many consecutive Redis errors. Attempting to reconnect...");
                    redis_connection = connect_to_redis_with_retry(&redis_url).await;
                    consecutive_errors = 0; // Reset counter after reconnection
                }

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
