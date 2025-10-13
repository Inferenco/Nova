mod admin;
mod circle;
mod create_group;
mod dao;
mod docs;
mod error;
mod info;
mod middlewares;
mod migration;
mod pay_members;
mod pay_users;
mod purchase;
mod router;
mod state;
mod util;

use std::env;

use dotenvy::dotenv;
use router::router;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let server_domain = env::var("SERVER_DOMAIN").unwrap_or("localhost".to_string());

    let app = router().await;

    let listener = tokio::net::TcpListener::bind(&server_domain).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
