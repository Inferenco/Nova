use std::env;

use anyhow::Result;
use aptos_rust_sdk_types::api_types::view::ViewRequest;
use quark_core::helpers::jwt::JwtManager;
use serde_json::Value;
use sled::Tree;
use teloxide::types::ChatId;

use crate::{group::dto::GroupCredentials, panora::handler::Panora};

#[derive(Clone)]
pub struct Group {
    pub jwt_manager: JwtManager,
    pub db: Tree,
    pub account_seed: String,
}

impl Group {
    pub fn new(db: Tree) -> Self {
        let jwt_manager = JwtManager::new();

        let account_seed: String =
            env::var("ACCOUNT_SEED").expect("ACCOUNT_SEED environment variable not found");

        Self {
            jwt_manager,
            db,
            account_seed,
        }
    }

    pub fn save_credentials(&self, credentials: GroupCredentials) -> Result<()> {
        let bytes = serde_json::to_vec(&credentials).unwrap();

        self.db
            .fetch_and_update(credentials.group_id.to_string(), |existing| {
                if let Some(existing) = existing {
                    let mut existing: GroupCredentials = serde_json::from_slice(existing).unwrap();
                    existing.jwt = credentials.jwt.clone();
                    existing.users = credentials.users.clone();

                    if existing.resource_account_address.is_empty()
                        || (!credentials.resource_account_address.is_empty()
                            && existing.resource_account_address
                                != credentials.resource_account_address)
                    {
                        existing.resource_account_address =
                            credentials.resource_account_address.clone();
                    }

                    return Some(serde_json::to_vec(&existing).unwrap());
                }

                Some(bytes.clone())
            })
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(())
    }

    pub fn generate_new_jwt(&self, group_id: ChatId) -> bool {
        let group_id = format!("{}-{}", group_id, self.account_seed);

        match self.jwt_manager.generate_group_token(group_id.clone()) {
            Ok(token) => {
                let jwt = token;

                let users: Vec<String> = vec![];

                let credentials =
                    GroupCredentials::from((jwt, group_id.clone(), "".to_string(), users));

                let saved = self.save_credentials(credentials);

                if saved.is_err() {
                    println!("❌ Failed to save credentials: {}", saved.err().unwrap());
                    return false;
                }

                println!("✅ Generated new JWT token for group {}", group_id);
                return true;
            }
            Err(e) => {
                println!("❌ Failed to generate JWT token: {}", e);
                return false;
            }
        }
    }

    pub fn get_credentials(&self, group_id: ChatId) -> Option<GroupCredentials> {
        let group_id = format!("{}-{}", group_id, self.account_seed);

        let bytes = self.db.get(group_id).unwrap();

        if let Some(bytes) = bytes {
            let credentials: GroupCredentials = serde_json::from_slice(&bytes).unwrap();
            Some(credentials)
        } else {
            None
        }
    }

    pub async fn group_exists(&self, group_id: ChatId, panora: Panora) -> bool {
        let group_id = format!("{}-{}", group_id, self.account_seed);

        let node = panora.aptos.node;

        let contract_address = panora.aptos.contract_address;

        let payload = ViewRequest {
            function: format!("{}::group::exist_group_id", contract_address),
            type_arguments: vec![],
            arguments: vec![Value::String(group_id)],
        };

        let response = node.view_function(payload).await;

        if response.is_err() {
            return false;
        }

        let response = response.unwrap().into_inner();

        let response = serde_json::from_value::<Vec<bool>>(response);

        if response.is_err() {
            return false;
        }

        let response = response.unwrap();

        response[0]
    }

    pub async fn verify(&self, group: ChatId) -> bool {
        let group_id = format!("{}-{}", group, self.account_seed);

        let credentials_opt = self.get_credentials(group);

        if let Some(credentials) = credentials_opt {
            // Initialize JWT manager and validate/update storage
            match self
                .jwt_manager
                .validate_and_update_group_jwt(credentials.jwt, group_id)
            {
                Ok(_updated_storage) => {
                    // Note: The updated storage with the new JWT would need to be
                    // persisted back to the dialogue storage in the calling code
                    return true;
                }
                Err(e) => {
                    log::warn!("AUTH: Failed to validate/generate JWT: {}", e);
                }
            }

            return self.generate_new_jwt(group);
        }

        println!("❌ No credentials found for group {}", group);
        return false;
    }

    pub async fn add_user_to_group(&self, group_id: ChatId, username: String) -> Result<()> {
        let credentials = self.get_credentials(group_id);

        if let Some(credentials) = credentials {
            let mut users = credentials.users;
            users.push(username);

            let new_credentials = GroupCredentials {
                jwt: credentials.jwt,
                group_id: credentials.group_id,
                resource_account_address: credentials.resource_account_address,
                users,
            };

            self.save_credentials(new_credentials)?;
        } else {
            return Err(anyhow::anyhow!(
                "No credentials found for group {}",
                group_id
            ));
        }

        Ok(())
    }
}
