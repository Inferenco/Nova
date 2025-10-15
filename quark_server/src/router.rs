use std::{env, str::FromStr, sync::Arc, time::Duration};

use aptos_rust_sdk::client::{builder::AptosClientBuilder, config::AptosNetwork};
use aptos_rust_sdk_types::api_types::{address::AccountAddress, chain_id::ChainId};
use axum::{
    Router, middleware,
    routing::{get, post},
};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_redoc::{Redoc, Servable};

use crate::{
    circle::{
        create_group_wallet::handler::create_group_wallet,
        create_user_wallet::handler::create_user_wallet,
    },
    create_group::handler::create_group,
    dao::handler::create_proposal,
    docs::{dto::ApiDoc, handler::api_docs},
    info::handler::info,
    middlewares::handler::{auth, auth_group},
    migration::handler::migrate_group_id,
    pay_members::handler::pay_members,
    pay_users::handler::pay_users,
    purchase::handler::{group_purchase, purchase},
    state::ServerState,
};

use inf_circle_sdk::circle_ops::circler_ops::CircleOps;
use redis::Client;

async fn connect_to_redis_with_retry(redis_url: &str) -> redis::aio::MultiplexedConnection {
    let mut retry_count = 0;
    let max_retries = 10;
    let base_delay = Duration::from_secs(1);

    loop {
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
                    println!(
                        "Failed to get Redis connection (attempt {}): {}",
                        retry_count + 1,
                        e
                    );
                }
            },
            Err(e) => {
                println!(
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

pub async fn router() -> Router {
    let network = env::var("APTOS_NETWORK").expect("APTOS_NETWORK environment variable not set");
    let contract_address =
        env::var("CONTRACT_ADDRESS").expect("CONTRACT_ADDRESS environment variable not set");
    let redis_url = env::var("REDIS_URL").expect("REDIS_URL environment variable not set");
    let aptos_api_key = env::var("APTOS_API_KEY").unwrap_or_default();
    let circle_wallet_set_id = env::var("CIRCLE_WALLET_SET_ID")
        .expect("CIRCLE_WALLET_SET_ID environment variable not set");
    let circle_ops = CircleOps::new()
        .map_err(|e| log::error!("Failed to create CircleOps: {}", e))
        .unwrap();

    println!("Attempting to connect to Redis");
    let redis_connection = connect_to_redis_with_retry(&redis_url).await;

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

    let state = Arc::new(ServerState::from((
        node,
        chain_id,
        contract_address,
        redis_connection,
        circle_ops,
        circle_wallet_set_id,
    )));

    let doc = ApiDoc::openapi();

    let auth_router = Router::new()
        .route("/pay-users", post(pay_users))
        .route("/purchase", post(purchase))
        .route_layer(middleware::from_fn(auth));

    let auth_group_router = Router::new()
        .route("/pay-members", post(pay_members))
        .route("/group-purchase", post(group_purchase))
        .route("/proposal", post(create_proposal))
        .route("/migrate-group-id", post(migrate_group_id))
        .route_layer(middleware::from_fn(auth_group));

    let circle_router = Router::new()
        .route("/circle/create-user-wallet", post(create_user_wallet))
        .route("/circle/create-group-wallet", post(create_group_wallet))
        .route_layer(middleware::from_fn(auth));

    Router::new()
        .merge(Redoc::with_url("/redoc", doc))
        .merge(auth_router)
        .route("/create-group", post(create_group))
        .merge(auth_group_router)
        .merge(circle_router)
        .route("/", get(info))
        .route("/docs", get(api_docs))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
