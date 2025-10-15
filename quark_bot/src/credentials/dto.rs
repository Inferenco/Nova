use inf_circle_sdk::dev_wallet::dto::DevWallet;
use serde::{Deserialize, Serialize};
use teloxide::types::UserId;

#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub jwt: String,
    pub user_id: UserId,
    pub account_address: String,
    pub resource_account_address: String,
    pub circle_wallets: Option<Vec<DevWallet>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CredentialsPayload {
    #[serde(rename = "accountAddress")]
    pub account_address: String,
    #[serde(rename = "resourceAccountAddress")]
    pub resource_account_address: String,
    #[serde(rename = "circleWallets")]
    pub circle_wallets: Option<Vec<DevWallet>>,
}

impl From<(String, UserId, String, String, Option<Vec<DevWallet>>)> for Credentials {
    fn from(value: (String, UserId, String, String, Option<Vec<DevWallet>>)) -> Self {
        let (jwt, user_id, account_address, resource_account_address, circle_wallets) = value;

        Credentials {
            jwt,
            user_id,
            account_address,
            resource_account_address,
            circle_wallets: circle_wallets,
        }
    }
}
