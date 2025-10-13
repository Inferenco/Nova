use std::sync::Arc;

use aptos_rust_sdk_types::api_types::chain_id::ChainId;
use axum::{Extension, Json, extract::State, http::StatusCode};
use inf_circle_sdk::{
    dev_wallet::{
        dto::{AccountType, DevWalletsResponse},
        ops::create_dev_wallet::CreateDevWalletRequestBuilder,
    },
    types::Blockchain,
};

use quark_core::helpers::dto::UserPayload;

use crate::{error::ErrorServer, state::ServerState};

#[utoipa::path(
    post,
    path = "/create-user-wallet",
    description = "Create user wallet",
    responses(
        (status = 200, description = "Successful Response"),
        (status = 400, description = "Bad Request"),
    )
)]
pub async fn create_wallet(
    State(server_state): State<Arc<ServerState>>,
    Extension(user): Extension<UserPayload>,
) -> Result<Json<DevWalletsResponse>, ErrorServer> {
    let circle_ops = server_state.circle_ops();
    let chain_id = server_state.chain_id();

    let wallet_set_id = server_state.circle_wallet_set_id();

    let blockchains = if chain_id == ChainId::Mainnet {
        vec![
            Blockchain::Aptos,
            Blockchain::Sol,
            Blockchain::Evm,
            Blockchain::Near,
        ]
    } else {
        vec![
            Blockchain::AptosTestnet,
            Blockchain::SolDevnet,
            Blockchain::EvmTestnet,
            Blockchain::NearTestnet,
        ]
    };

    let create_wallet_request =
        CreateDevWalletRequestBuilder::new(wallet_set_id.clone(), blockchains)
            .map_err(|e| ErrorServer {
                status: StatusCode::BAD_REQUEST.into(),
                message: e.to_string(),
            })?
            .account_type(AccountType::Eoa)
            .ref_id(format!("nova-user-{}", user.account_address))
            .name(format!("nova-wallet-{}", user.account_address));

    let wallet = circle_ops
        .create_dev_wallet(create_wallet_request)
        .await
        .map_err(|e| ErrorServer {
            status: StatusCode::INTERNAL_SERVER_ERROR.into(),
            message: e.to_string(),
        })?;

    Ok(Json(wallet))
}
