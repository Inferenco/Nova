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
    create_group::handler::create_group,
    docs::{dto::ApiDoc, handler::api_docs},
    info::handler::info,
    middlewares::handler::{auth, auth_group},
    pay_members::handler::pay_members,
    pay_users::handler::pay_users,
    purchase::handler::{group_purchase, purchase},
    state::ServerState,
};

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

    println!("Attempting to connect to Redis at: {}", redis_url);
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
            ChainId::Testing,
        ),
        _ => (
            AptosClientBuilder::new(AptosNetwork::testnet()),
            ChainId::Testnet,
        ),
    };

    let node = builder.build();

    let contract_address = AccountAddress::from_str(&contract_address)
        .expect("CONTRACT_ADDRESS is not a valid account address");

    let state = Arc::new(ServerState::from((
        node,
        chain_id,
        contract_address,
        redis_connection,
    )));

    let doc = ApiDoc::openapi();

    let auth_router = Router::new()
        .route("/pay-users", post(pay_users))
        .route("/purchase", post(purchase))
        .route_layer(middleware::from_fn(auth));

    let auth_group_router = Router::new()
        .route("/pay-members", post(pay_members))
        .route("/group-purchase", post(group_purchase))
        .route_layer(middleware::from_fn(auth_group));

    Router::new()
        .merge(Redoc::with_url("/redoc", doc))
        .merge(auth_router)
        .route("/create-group", post(create_group))
        .merge(auth_group_router)
        .route("/", get(info))
        .route("/docs", get(api_docs))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
