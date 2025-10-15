use crate::{credentials::dto::Credentials, services::handler::Services};
use anyhow::Result;
use inf_circle_sdk::{
    circle_view::circle_view::CircleView,
    dev_wallet::{dto::DevWallet, views::list_wallets::ListDevWalletsParamsBuilder},
};
use quark_core::helpers::jwt::JwtManager;
use serde_json;
use sled::Tree;
use teloxide::types::{Message, UserId};

#[derive(Clone)]
pub struct Auth {
    jwt_manager: JwtManager,
    circle_view: CircleView,
    db: Tree,
    wallet_set_id: String,
    services: Services,
}

impl Auth {
    pub fn new(
        db: Tree,
        circle_view: CircleView,
        wallet_set_id: String,
        services: Services,
    ) -> Self {
        let jwt_manager = JwtManager::new();

        Self {
            jwt_manager,
            db,
            circle_view,
            wallet_set_id,
            services,
        }
    }

    pub fn get_credentials(&self, username: &str) -> Option<Credentials> {
        let bytes_op = self.db.get(username).unwrap();

        if let Some(bytes) = bytes_op {
            let credentials: Credentials = serde_json::from_slice(&bytes).unwrap();
            Some(credentials)
        } else {
            None
        }
    }

    pub fn save_credentials(&self, username: &str, credentials: Credentials) -> Result<()> {
        let bytes = serde_json::to_vec(&credentials).unwrap();
        self.db
            .insert(username, bytes)
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(())
    }

    pub async fn generate_new_jwt(
        &self,
        username: String,
        user_id: UserId,
        account_address: String,
        resource_account_address: String,
        circle_wallets: Option<Vec<DevWallet>>,
    ) -> bool {
        let account_address = account_address.clone();

        match self
            .jwt_manager
            .generate_token(user_id, account_address.clone())
        {
            Ok(token) => {
                let jwt = token.clone();

                let circle_wallets = if circle_wallets.is_some() {
                    circle_wallets
                } else {
                    self.get_circle_wallets(account_address.clone(), token.clone())
                        .await
                };

                let credentials = Credentials::from((
                    jwt,
                    user_id,
                    account_address,
                    resource_account_address,
                    circle_wallets,
                ));

                let saved = self.save_credentials(&username, credentials);

                if saved.is_err() {
                    println!("❌ Failed to save credentials: {}", saved.err().unwrap());
                    return false;
                }

                println!("✅ Generated new JWT token for user {}", user_id);
                return true;
            }
            Err(e) => {
                println!("❌ Failed to generate JWT token: {}", e);
                return false;
            }
        }
    }

    pub async fn verify(&self, msg: Message) -> bool {
        let user = msg.from;

        if user.is_none() {
            return false;
        }

        let user = user.unwrap();

        let username = user.username;

        if username.is_none() {
            return false;
        }

        let username = username.unwrap();

        let credentials_opt = self.get_credentials(&username);

        if let Some(credentials) = credentials_opt {
            // Initialize JWT manager and validate/update storage
            match self.jwt_manager.validate_and_update_jwt(
                credentials.jwt,
                credentials.user_id,
                credentials.account_address.clone(),
            ) {
                Ok(_updated_storage) => {
                    // Note: The updated storage with the new JWT would need to be
                    // persisted back to the dialogue storage in the calling code
                    return true;
                }
                Err(e) => {
                    log::warn!("AUTH: Failed to validate/generate JWT: {}", e);
                }
            }

            return self
                .generate_new_jwt(
                    username,
                    user.id,
                    credentials.account_address,
                    credentials.resource_account_address,
                    credentials.circle_wallets,
                )
                .await;
        }

        println!("❌ No credentials found for user {}", username);
        return false;
    }

    pub fn get_all_users(&self) -> Result<Vec<Credentials>> {
        let users = self
            .db
            .iter()
            .map(|result| {
                let (_, value) = result?;
                let credentials: Credentials = serde_json::from_slice(&value).unwrap();
                Ok(credentials)
            })
            .collect::<Result<Vec<Credentials>>>();

        users
    }

    async fn get_circle_wallets(
        &self,
        account_address: String,
        token: String,
    ) -> Option<Vec<DevWallet>> {
        let params = ListDevWalletsParamsBuilder::new()
            .wallet_set_id(self.wallet_set_id.clone())
            .ref_id(format!("nova-user-{}", account_address));

        let circle_wallets = self.circle_view.list_wallets(params.build()).await;

        match circle_wallets {
            Ok(wallets) if !wallets.wallets.is_empty() => Some(wallets.wallets),
            _ => self
                .services
                .create_user_wallet(token)
                .await
                .ok()
                .map(|wallets| wallets.wallets),
        }
    }
}
