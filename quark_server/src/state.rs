use aptos_rust_sdk::client::rest_api::AptosFullnodeClient;
use aptos_rust_sdk_types::api_types::{address::AccountAddress, chain_id::ChainId};
use inf_circle_sdk::circle_ops::circler_ops::CircleOps;
use redis::aio::MultiplexedConnection;

#[derive(Clone)]
pub struct ServerState {
    node: AptosFullnodeClient,
    chain_id: ChainId,
    contract_address: AccountAddress,
    redis_client: MultiplexedConnection,
    circle_ops: CircleOps,
    circle_wallet_set_id: String,
}

impl
    From<(
        AptosFullnodeClient,
        ChainId,
        AccountAddress,
        MultiplexedConnection,
        CircleOps,
        String,
    )> for ServerState
{
    fn from(
        states: (
            AptosFullnodeClient,
            ChainId,
            AccountAddress,
            MultiplexedConnection,
            CircleOps,
            String,
        ),
    ) -> Self {
        let (node, chain_id, contract_address, redis_client, circle_ops, circle_wallet_set_id) =
            states;
        Self {
            node,
            chain_id,
            contract_address,
            redis_client,
            circle_ops,
            circle_wallet_set_id,
        }
    }
}

impl ServerState {
    pub fn node(&self) -> &AptosFullnodeClient {
        &self.node
    }

    pub fn chain_id(&self) -> ChainId {
        self.chain_id
    }

    pub fn contract_address(&self) -> AccountAddress {
        self.contract_address
    }

    pub fn redis_client(&self) -> &MultiplexedConnection {
        &self.redis_client
    }

    pub fn circle_ops(&self) -> &CircleOps {
        &self.circle_ops
    }

    pub fn circle_wallet_set_id(&self) -> &String {
        &self.circle_wallet_set_id
    }
}
