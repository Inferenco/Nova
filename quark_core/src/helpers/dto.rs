use open_ai_rust_responses_by_sshift::Model;
use serde::{Deserialize, Serialize};
use std::{env, fmt, str::FromStr};
use teloxide::types::UserId;
use utoipa::ToSchema;

pub enum Endpoints {
    CreateGroup,
    PayUsers,
    Purchase,
    PayMembers,
    GroupPurchase,
    CreateProposal,
    MigrateGroupId,
    CreateUserWallet,
    CreateGroupWallet,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub telegram_id: UserId,
    pub exp: i64, // Expiration time
    pub iat: i64, // Issued at
    pub account_address: String,
    pub group_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupClaims {
    pub group_id: String,
    pub exp: i64, // Expiration time
    pub iat: i64, // Issued at
}

#[derive(Debug, Clone)]
pub struct UserPayload {
    pub account_address: String,
}

#[derive(Debug, Clone)]
pub struct GroupPayload {
    pub group_id: String,
}

#[derive(Deserialize, Serialize, Debug, ToSchema)]
pub struct PurchaseRequest {
    #[schema(value_type = String)]
    pub model: Model,
    pub currency: String,
    pub coin_version: CoinVersion,
    pub tokens_used: u32,
    pub tools_used: Vec<ToolUsage>,
    pub group_id: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, ToSchema)]
pub struct PurchaseMessage {
    #[schema(value_type = String)]
    pub model: Model,
    pub currency: String,
    pub coin_version: CoinVersion,
    pub tokens_used: u32,
    pub tools_used: Vec<ToolUsage>,
    pub account_address: String,
    pub group_id: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, ToSchema)]
pub struct ToolUsage {
    pub tool: AITool,
    pub calls: u32,
}

#[derive(Deserialize, Serialize, Debug, ToSchema)]
pub enum AITool {
    FileSearch,
    ImageGeneration,
    WebSearchPreview,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub enum CoinVersion {
    V1,
    V2,
}

#[derive(Deserialize, Serialize, Debug, ToSchema)]
pub struct PayUsersRequest {
    pub amount: u64,
    pub users: Vec<String>,
    pub coin_type: String,
    pub version: CoinVersion,
}

#[derive(Deserialize, Serialize, Debug, ToSchema)]
pub struct TransactionResponse {
    pub hash: String,
}

#[derive(Deserialize, Serialize, Debug, ToSchema)]
pub struct CreateGroupRequest {
    pub group_id: String,
}

#[derive(Deserialize, Debug)]
pub struct SimulateTransactionResponse {
    pub success: bool,
    pub vm_status: String,
}

#[derive(Debug, Deserialize)]
pub struct TokenAddress {
    pub vec: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceCoin {
    pub chain_id: Option<u64>,
    pub panora_id: Option<String>,
    pub token_address: Option<String>,
    pub fa_address: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
    pub usd_price: Option<String>,
    pub native_price: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, ToSchema)]
pub struct CreateProposalRequest {
    pub name: String,
    pub description: String,
    pub options: Vec<String>,
    pub start_date: u64,
    pub end_date: u64,
    pub proposal_id: String,
    pub version: CoinVersion,
    pub currency: String,
    pub thread_id: Option<i32>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct GasPrice {
    pub deprioritized_gas_estimate: u64,
    pub gas_estimate: u64,
    pub prioritized_gas_estimate: u64,
}

impl fmt::Display for Endpoints {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let backend_host =
            env::var("BACKEND_URL").expect("BACKEND_URL environment variable not set");
        let backend_url = backend_host;

        match self {
            &Endpoints::PayUsers => write!(f, "{}/pay-users", backend_url),
            &Endpoints::Purchase => write!(f, "{}/purchase", backend_url),
            &Endpoints::PayMembers => write!(f, "{}/pay-members", backend_url),
            &Endpoints::CreateGroup => write!(f, "{}/create-group", backend_url),
            &Endpoints::GroupPurchase => write!(f, "{}/group-purchase", backend_url),
            &Endpoints::CreateProposal => write!(f, "{}/proposal", backend_url),
            &Endpoints::MigrateGroupId => write!(f, "{}/migrate-group-id", backend_url),
            &Endpoints::CreateUserWallet => write!(f, "{}/circle/create-user-wallet", backend_url),
            &Endpoints::CreateGroupWallet => {
                write!(f, "{}/circle/create-group-wallet", backend_url)
            }
        }
    }
}

impl fmt::Display for CoinVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoinVersion::V1 => write!(f, "V1"),
            CoinVersion::V2 => write!(f, "V2"),
        }
    }
}

impl FromStr for CoinVersion {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "V1" => Ok(CoinVersion::V1),
            "V2" => Ok(CoinVersion::V2),
            _ => Err(format!("Invalid coin version: {}", s)),
        }
    }
}

impl From<(PurchaseRequest, String)> for PurchaseMessage {
    fn from((request, account_address): (PurchaseRequest, String)) -> Self {
        let model = request.model;
        let tokens_used = request.tokens_used;
        let tools_used = request.tools_used;
        let group_id = request.group_id;
        let currency = request.currency;
        let coin_version = request.coin_version;

        PurchaseMessage {
            model,
            tokens_used,
            tools_used,
            account_address,
            group_id,
            currency,
            coin_version,
        }
    }
}
