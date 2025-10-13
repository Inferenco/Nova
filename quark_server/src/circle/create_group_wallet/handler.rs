use std::sync::Arc;

use axum::{Extension, Json, extract::State, http::StatusCode};
use inf_circle_sdk::{
    dev_wallet::{
        dto::{AccountType, DevWalletsResponse},
        ops::create_dev_wallet::CreateDevWalletRequestBuilder,
    },
    helper::CircleResponse,
    types::Blockchain,
};
use quark_core::helpers::dto::{GroupPayload, UserPayload};

use crate::{error::ErrorServer, state::ServerState};

#[utoipa::path(
    post,
    path = "/create-group-wallet",
    description = "Create group wallet",
    responses(
        (status = 200, body = CircleResponse<DevWalletsResponse>, description = "Successful Response"),
        (status = 400, description = "Bad Request"),
    )
)]
pub async fn create_group_wallet(
    State(server_state): State<Arc<ServerState>>,
    Extension(group): Extension<GroupPayload>,
) -> Result<Json<CircleResponse<DevWalletsResponse>>, ErrorServer> {
    let circle_ops = server_state.circle_ops();
    let chain_id = server_state.chain_id();

    let wallet_set_id = server_state.circle_wallet_set_id();

    let blockchains = if chain_id == ChainId::Mainnet {
        vec![Blockchain::Aptos, Blockchain::Evm, Blockchain::Near]
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
            .ref_id(format!("nova-group-{}", group.group_id))
            .name(format!("nova-group-{}", group.group_id));

    let wallet = circle_ops
        .create_dev_wallet(create_wallet_request)
        .await
        .map_err(|e| ErrorServer {
            status: StatusCode::INTERNAL_SERVER_ERROR.into(),
            message: e.to_string(),
        })?;

    Ok(Json(CircleResponse::new(wallet)))
}
