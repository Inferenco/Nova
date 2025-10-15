use inf_circle_sdk::dev_wallet::dto::DevWallet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupCredentials {
    pub jwt: String,
    pub group_id: String,
    pub resource_account_address: String,
    pub users: Vec<String>,
    pub circle_wallets: Option<Vec<DevWallet>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupCredentialsPayload {
    #[serde(rename = "resourceAccountAddress")]
    pub resource_account_address: String,
}

impl From<(String, String, String, Vec<String>, Option<Vec<DevWallet>>)> for GroupCredentials {
    fn from(value: (String, String, String, Vec<String>, Option<Vec<DevWallet>>)) -> Self {
        let (jwt, group_id, resource_account_address, users, circle_wallets) = value;

        GroupCredentials {
            jwt,
            group_id,
            resource_account_address,
            users,
            circle_wallets,
        }
    }
}
