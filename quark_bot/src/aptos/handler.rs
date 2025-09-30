use anyhow::Result;
use aptos_rust_sdk::client::{
    builder::AptosClientBuilder, config::AptosNetwork, rest_api::AptosFullnodeClient,
};
use aptos_rust_sdk_types::api_types::{chain_id::ChainId, view::ViewRequest};
use quark_core::helpers::dto::TokenAddress;

#[derive(Clone)]
pub struct Aptos {
    pub node: AptosFullnodeClient,
    pub contract_address: String,
}

impl Aptos {
    pub fn new(network: String, contract_address: String, api_key: String) -> Self {
        let (builder, _chain_id) = match network.as_str() {
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

        let node = if api_key.is_empty() {
            log::info!("Building node without API key");
            builder.build()
        } else {
            log::info!("Building node with API key");
            builder.api_key(api_key.as_str()).unwrap().build()
        };

        Self {
            node,
            contract_address,
        }
    }

    pub async fn get_token_address(&self) -> Result<String> {
        const MAX_RETRIES: u32 = 3;
        const BASE_DELAY_MS: u64 = 2000; // 2 seconds base delay

        for attempt in 1..=MAX_RETRIES {
            match self.get_token_address_internal().await {
                Ok(address) => return Ok(address),
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("429") && attempt < MAX_RETRIES {
                        // Exponential backoff: 2s, 4s, 8s
                        let delay_ms = BASE_DELAY_MS * (2_u64.pow(attempt - 1));
                        log::warn!(
                            "Rate limited when getting token address (attempt {}/{}), waiting {}ms before retry",
                            attempt,
                            MAX_RETRIES,
                            delay_ms
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "Failed to get token address after {} retries",
            MAX_RETRIES
        ))
    }

    pub async fn get_fees_currency_payment_list(&self) -> Result<Vec<String>> {
        let fees_currency_payment_list = self
            .node
            .view_function(ViewRequest {
                function: format!(
                    "{}::admin::get_fees_currency_payment_list",
                    self.contract_address
                ),
                type_arguments: vec![],
                arguments: vec![],
            })
            .await?
            .into_inner();

        let fees_currency_payment_list =
            serde_json::from_value::<Vec<Vec<String>>>(fees_currency_payment_list)?;

        Ok(fees_currency_payment_list[0].clone())
    }

    async fn get_token_address_internal(&self) -> Result<String> {
        let coin_address_value = self
            .node
            .view_function(ViewRequest {
                function: format!("{}::user::get_token_address", self.contract_address),
                type_arguments: vec![],
                arguments: vec![],
            })
            .await?
            .into_inner();

        let coin_address = serde_json::from_value::<Vec<TokenAddress>>(coin_address_value)?;

        if coin_address[0].vec[0].clone() == "0x1" {
            return Ok(format!("0x1::aptos_coin::AptosCoin"));
        }

        Ok(coin_address[0].vec[0].clone())
    }

    pub async fn get_account_balance(&self, address: &str, token_address: &str) -> Result<i64> {
        let coin_address_formatted = if token_address == String::from("0x1") {
            format!("0x1::aptos_coin::AptosCoin")
        } else {
            token_address.to_string()
        };

        let balance = self
            .node
            .get_account_balance(address.to_string(), coin_address_formatted)
            .await?;

        let balance = balance.into_inner();

        let balance = balance.as_i64();

        if balance.is_none() {
            return Err(anyhow::anyhow!("Balance not found"));
        }

        Ok(balance.unwrap())
    }
}
