// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use base64::Engine;
use cid::multihash::MultihashDigest;
use cid::Cid;
use fvm_ipld_encoding::{to_vec, RawBytes};
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::crypto::signature::Signature;
use fvm_shared::econ::TokenAmount;
use ipc_api::subnet_id::SubnetID;
use ipc_wallet::Wallet;
use num_traits::cast::ToPrimitive;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::jsonrpc::{JsonRpcClient, JsonRpcClientImpl, NO_PARAMS};
use crate::lotus::message::chain::{ChainHeadResponse, GetTipSetByHeightResponse};
use crate::lotus::message::mpool::{
    EstimateGasResponse, MpoolPushMessage, MpoolPushMessageResponse, MpoolPushMessageResponseInner,
};
use crate::lotus::message::state::{ReadStateResponse, StateWaitMsgResponse};
use crate::lotus::message::wallet::{WalletKeyType, WalletListResponse};
use crate::lotus::message::CIDMap;
use crate::lotus::{LotusClient, NetworkVersion};

pub type DefaultLotusJsonRPCClient = LotusJsonRPCClient<JsonRpcClientImpl>;

// RPC methods
mod methods {
    pub const MPOOL_PUSH_MESSAGE: &str = "Filecoin.MpoolPushMessage";
    pub const MPOOL_PUSH: &str = "Filecoin.MpoolPush";
    pub const MPOOL_GET_NONCE: &str = "Filecoin.MpoolGetNonce";
    pub const STATE_WAIT_MSG: &str = "Filecoin.StateWaitMsg";
    pub const STATE_NETWORK_NAME: &str = "Filecoin.StateNetworkName";
    pub const STATE_NETWORK_VERSION: &str = "Filecoin.StateNetworkVersion";
    pub const STATE_ACTOR_CODE_CIDS: &str = "Filecoin.StateActorCodeCIDs";
    pub const WALLET_NEW: &str = "Filecoin.WalletNew";
    pub const WALLET_LIST: &str = "Filecoin.WalletList";
    pub const WALLET_BALANCE: &str = "Filecoin.WalletBalance";
    pub const WALLET_DEFAULT_ADDRESS: &str = "Filecoin.WalletDefaultAddress";
    pub const STATE_READ_STATE: &str = "Filecoin.StateReadState";
    pub const CHAIN_HEAD: &str = "Filecoin.ChainHead";
    pub const GET_TIPSET_BY_HEIGHT: &str = "Filecoin.ChainGetTipSetByHeight";
    pub const ESTIMATE_MESSAGE_GAS: &str = "Filecoin.GasEstimateMessageGas";
}

/// The default state wait confidence value
/// NOTE: we can afford 0 epochs confidence (and even one)
/// with instant-finality consensus, but with Filecoin mainnet this should be increased
/// in case there are reorgs.
const STATE_WAIT_CONFIDENCE: u8 = 0;
/// We dont set a limit on the look back epoch, i.e. check against latest block
const STATE_WAIT_LOOK_BACK_NO_LIMIT: i8 = -1;
/// We are not replacing any previous messages.
/// TODO: when set to false, lotus raises `found message with equal nonce as the one we are looking`
/// TODO: error. Should check this again.
const STATE_WAIT_ALLOW_REPLACE: bool = true;

/// The struct implementation for Lotus Client API. It allows for multiple different trait
/// extension.
/// # Examples
/// ```no_run
/// use ipc_provider::{lotus::LotusClient, lotus::client::LotusJsonRPCClient};
/// use ipc_provider::jsonrpc::JsonRpcClientImpl;
/// use ipc_api::subnet_id::SubnetID;
///
/// #[tokio::main]
/// async fn main() {
///     let h = JsonRpcClientImpl::new("<DEFINE YOUR URL HERE>".parse().unwrap(), None);
///     let n = LotusJsonRPCClient::new(h, SubnetID::default());
///     println!(
///         "wallets: {:?}",
///         n.wallet_new(ipc_provider::lotus::message::wallet::WalletKeyType::Secp256k1).await
///     );
/// }
/// ```
pub struct LotusJsonRPCClient<T: JsonRpcClient> {
    client: T,
    subnet: SubnetID,
    wallet_store: Option<Arc<RwLock<Wallet>>>,
}

impl<T: JsonRpcClient> LotusJsonRPCClient<T> {
    pub fn new(client: T, subnet: SubnetID) -> Self {
        Self {
            client,
            subnet,
            wallet_store: None,
        }
    }

    pub fn new_with_wallet_store(
        client: T,
        subnet: SubnetID,
        wallet_store: Arc<RwLock<Wallet>>,
    ) -> Self {
        Self {
            client,
            subnet,
            wallet_store: Some(wallet_store),
        }
    }
}

#[async_trait]
impl<T: JsonRpcClient + Send + Sync> LotusClient for LotusJsonRPCClient<T> {
    async fn mpool_push_message(
        &self,
        msg: MpoolPushMessage,
    ) -> Result<MpoolPushMessageResponseInner> {
        let nonce = msg
            .nonce
            .map(|n| serde_json::Value::Number(n.into()))
            .unwrap_or(serde_json::Value::Null);

        let to_value = |t: Option<TokenAmount>| {
            t.map(|n| serde_json::Value::Number(n.atto().to_u64().unwrap().into()))
                .unwrap_or(serde_json::Value::Null)
        };
        let gas_limit = to_value(msg.gas_limit);
        let gas_premium = to_value(msg.gas_premium);
        let gas_fee_cap = to_value(msg.gas_fee_cap);
        let max_fee = to_value(msg.max_fee);

        // refer to: https://lotus.filecoin.io/reference/lotus/mpool/#mpoolpushmessage
        let params = json!([
            {
                "to": msg.to.to_string(),
                "from": msg.from.to_string(),
                "value": msg.value.atto().to_string(),
                "method": msg.method,
                "params": msg.params,

                // THESE ALL WILL AUTO POPULATE if null
                "nonce": nonce,
                "gas_limit": gas_limit,
                "gas_fee_cap": gas_fee_cap,
                "gas_premium": gas_premium,
                "cid": CIDMap::from(msg.cid),
                "version": serde_json::Value::Null,
            },
            {
                "max_fee": max_fee
            }
        ]);

        let r = self
            .client
            .request::<MpoolPushMessageResponse>(methods::MPOOL_PUSH_MESSAGE, params)
            .await?;
        tracing::debug!("received mpool_push_message response: {r:?}");

        Ok(r.message)
    }

    async fn mpool_push(&self, mut msg: MpoolPushMessage) -> Result<Cid> {
        if msg.nonce.is_none() {
            let nonce = self.mpool_nonce(&msg.from).await?;
            tracing::info!(
                "sender: {:} with nonce: {nonce:} in subnet: {:}",
                msg.from,
                self.subnet
            );
            msg.nonce = Some(nonce);
        }

        if msg.version.is_none() {
            msg.version = Some(0);
        }

        self.estimate_message_gas(&mut msg).await?;
        tracing::debug!("estimated gas for message: {msg:?}");

        let signature = self.sign_mpool_message(&msg)?;

        let params = create_signed_message_params(msg, signature);
        tracing::debug!(
            "message to push to mpool: {params:?} in subnet: {:?}",
            self.subnet
        );

        let r = self
            .client
            .request::<CIDMap>(methods::MPOOL_PUSH, params)
            .await?;
        tracing::debug!("received mpool_push_message response: {r:?}");

        Cid::try_from(r)
    }

    async fn state_wait_msg(&self, cid: Cid) -> Result<StateWaitMsgResponse> {
        // refer to: https://lotus.filecoin.io/reference/lotus/state/#statewaitmsg
        let params = json!([
            CIDMap::from(cid),
            STATE_WAIT_CONFIDENCE,
            STATE_WAIT_LOOK_BACK_NO_LIMIT,
            STATE_WAIT_ALLOW_REPLACE,
        ]);

        let r = self
            .client
            .request::<StateWaitMsgResponse>(methods::STATE_WAIT_MSG, params)
            .await?;
        tracing::debug!("received state_wait_msg response: {r:?}");
        Ok(r)
    }

    async fn state_network_name(&self) -> Result<String> {
        // refer to: https://lotus.filecoin.io/reference/lotus/state/#statenetworkname
        let r = self
            .client
            .request::<String>(methods::STATE_NETWORK_NAME, serde_json::Value::Null)
            .await?;
        tracing::debug!("received state_network_name response: {r:?}");
        Ok(r)
    }

    async fn state_network_version(&self, tip_sets: Vec<Cid>) -> Result<NetworkVersion> {
        // refer to: https://lotus.filecoin.io/reference/lotus/state/#statenetworkversion
        let params = json!([tip_sets.into_iter().map(CIDMap::from).collect::<Vec<_>>()]);

        let r = self
            .client
            .request::<NetworkVersion>(methods::STATE_NETWORK_VERSION, params)
            .await?;

        tracing::debug!("received state_network_version response: {r:?}");
        Ok(r)
    }

    async fn state_actor_code_cids(
        &self,
        network_version: NetworkVersion,
    ) -> Result<HashMap<String, Cid>> {
        // refer to: https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-v1-unstable-methods.md#stateactormanifestcid
        let params = json!([network_version]);

        let r = self
            .client
            .request::<HashMap<String, CIDMap>>(methods::STATE_ACTOR_CODE_CIDS, params)
            .await?;

        let mut cids = HashMap::new();
        for (key, cid_map) in r.into_iter() {
            cids.insert(key, Cid::try_from(cid_map)?);
        }

        tracing::debug!("received state_actor_manifest_cid response: {cids:?}");
        Ok(cids)
    }

    async fn wallet_default(&self) -> Result<Address> {
        // refer to: https://lotus.filecoin.io/reference/lotus/wallet/#walletdefaultaddress
        let r = self
            .client
            .request::<String>(methods::WALLET_DEFAULT_ADDRESS, json!({}))
            .await?;
        tracing::debug!("received wallet_default response: {r:?}");

        let addr = Address::from_str(&r)?;
        Ok(addr)
    }

    async fn wallet_list(&self) -> Result<WalletListResponse> {
        // refer to: https://lotus.filecoin.io/reference/lotus/wallet/#walletlist
        let r = self
            .client
            .request::<WalletListResponse>(methods::WALLET_LIST, json!({}))
            .await?;
        tracing::debug!("received wallet_list response: {r:?}");
        Ok(r)
    }

    async fn wallet_new(&self, key_type: WalletKeyType) -> Result<String> {
        let key_type_str = key_type.as_ref();
        // refer to: https://lotus.filecoin.io/reference/lotus/wallet/#walletnew
        let r = self
            .client
            .request::<String>(methods::WALLET_NEW, json!([key_type_str]))
            .await?;
        tracing::debug!("received wallet_new response: {r:?}");
        Ok(r)
    }

    async fn wallet_balance(&self, address: &Address) -> Result<TokenAmount> {
        // refer to: https://lotus.filecoin.io/reference/lotus/wallet/#walletbalance
        let r = self
            .client
            .request::<String>(methods::WALLET_BALANCE, json!([address.to_string()]))
            .await?;
        tracing::debug!("received wallet_balance response: {r:?}");

        let v = BigInt::from_str(&r)?;
        Ok(TokenAmount::from_atto(v))
    }

    async fn read_state<State: DeserializeOwned + Debug>(
        &self,
        address: Address,
        tipset: Cid,
    ) -> Result<ReadStateResponse<State>> {
        // refer to: https://lotus.filecoin.io/reference/lotus/state/#statereadstate
        let r = self
            .client
            .request::<ReadStateResponse<State>>(
                methods::STATE_READ_STATE,
                json!([address.to_string(), [CIDMap::from(tipset)]]),
            )
            .await?;
        tracing::debug!("received read_state response: {r:?}");
        Ok(r)
    }

    async fn chain_head(&self) -> Result<ChainHeadResponse> {
        let r = self
            .client
            .request::<ChainHeadResponse>(methods::CHAIN_HEAD, NO_PARAMS)
            .await?;
        tracing::debug!("received chain_head response: {r:?}");
        Ok(r)
    }

    async fn current_epoch(&self) -> Result<ChainEpoch> {
        Ok(self.chain_head().await?.height as ChainEpoch)
    }

    async fn get_tipset_by_height(
        &self,
        epoch: ChainEpoch,
        tip_set: Cid,
    ) -> Result<GetTipSetByHeightResponse> {
        let r = self
            .client
            .request::<GetTipSetByHeightResponse>(
                methods::GET_TIPSET_BY_HEIGHT,
                json!([epoch, [CIDMap::from(tip_set)]]),
            )
            .await?;
        tracing::debug!("received get_tipset_by_height response: {r:?}");
        Ok(r)
    }
}

impl<T: JsonRpcClient + Send + Sync> LotusJsonRPCClient<T> {
    fn sign_mpool_message(&self, msg: &MpoolPushMessage) -> anyhow::Result<Signature> {
        if self.wallet_store.is_none() {
            return Err(anyhow!("key store not set, function not supported"));
        }

        let message = fvm_shared::message::Message {
            version: msg
                .version
                .ok_or_else(|| anyhow!("version should not be empty"))? as u64,
            from: msg.from,
            to: msg.to,
            sequence: msg
                .nonce
                .ok_or_else(|| anyhow!("nonce should not be empty"))?,
            value: msg.value.clone(),
            method_num: msg.method,
            params: RawBytes::from(msg.params.clone()),
            gas_limit: msg
                .gas_limit
                .as_ref()
                .ok_or_else(|| anyhow!("gas_limit should not be empty"))?
                .atto()
                .to_u64()
                .unwrap(),
            gas_fee_cap: msg
                .gas_fee_cap
                .as_ref()
                .ok_or_else(|| anyhow!("gas_fee_cap should not be empty"))?
                .clone(),
            gas_premium: msg
                .gas_premium
                .as_ref()
                .ok_or_else(|| anyhow!("gas_premium should not be empty"))?
                .clone(),
        };

        let hash = cid::multihash::Code::Blake2b256.digest(&to_vec(&message)?);
        let msg_cid = Cid::new_v1(fvm_ipld_encoding::DAG_CBOR, hash).to_bytes();

        let mut wallet_store = self.wallet_store.as_ref().unwrap().write().unwrap();
        Ok(wallet_store.sign(&msg.from, &msg_cid)?)
    }

    async fn estimate_message_gas(&self, msg: &mut MpoolPushMessage) -> anyhow::Result<()> {
        let params = json!([
            {
                "Version": msg.version.unwrap_or(0),
                "To": msg.to.to_string(),
                "From": msg.from.to_string(),
                "Value": msg.value.atto().to_string(),
                "Method": msg.method,
                "Params": msg.params,
                "Nonce": msg.nonce,

                "GasLimit": 0,
                "GasFeeCap": "0",
                "GasPremium": "0",

                "CID": CIDMap::from(msg.cid),
            },
            {},
            []
        ]);

        let gas = self
            .client
            .request::<EstimateGasResponse>(methods::ESTIMATE_MESSAGE_GAS, params)
            .await?;

        msg.gas_fee_cap = gas.gas_fee_cap;
        msg.gas_limit = gas.gas_limit;
        msg.gas_premium = gas.gas_premium;

        Ok(())
    }

    async fn mpool_nonce(&self, address: &Address) -> anyhow::Result<u64> {
        let params = json!([address.to_string()]);
        let r = self
            .client
            .request::<u64>(methods::MPOOL_GET_NONCE, params)
            .await;
        if let Err(e) = r {
            if e.to_string().contains("resolution lookup failed") {
                return Ok(0);
            }
            return Err(e);
        }
        Ok(r.unwrap())
    }
}

impl LotusJsonRPCClient<JsonRpcClientImpl> {
    /// A constructor that returns a `LotusJsonRPCClient` from a `Subnet`. The returned
    /// `LotusJsonRPCClient` makes requests to the URL defined in the `Subnet`.
    pub fn from_subnet(subnet: &crate::config::Subnet) -> Self {
        let url = subnet.rpc_http().clone();
        let auth_token = subnet.auth_token();
        let jsonrpc_client = JsonRpcClientImpl::new(url, auth_token.as_deref());
        LotusJsonRPCClient::new(jsonrpc_client, subnet.id.clone())
    }

    pub fn from_subnet_with_wallet_store(
        subnet: &crate::config::Subnet,
        wallet_store: Arc<RwLock<Wallet>>,
    ) -> Self {
        let url = subnet.rpc_http().clone();
        let auth_token = subnet.auth_token();
        let jsonrpc_client = JsonRpcClientImpl::new(url, auth_token.as_deref());
        LotusJsonRPCClient::new_with_wallet_store(jsonrpc_client, subnet.id.clone(), wallet_store)
    }
}

fn create_signed_message_params(msg: MpoolPushMessage, signature: Signature) -> serde_json::Value {
    let nonce = msg
        .nonce
        .map(|n| serde_json::Value::Number(n.into()))
        .unwrap_or(serde_json::Value::Null);

    let to_value_str = |t: Option<TokenAmount>| {
        t.map(|n| serde_json::Value::String(n.atto().to_u64().unwrap().to_string()))
            .unwrap_or(serde_json::Value::Null)
    };
    let to_value = |t: Option<TokenAmount>| {
        t.map(|n| serde_json::Value::Number(n.atto().to_u64().unwrap().into()))
            .unwrap_or(serde_json::Value::Null)
    };

    let gas_limit = to_value(msg.gas_limit);
    let gas_premium = to_value_str(msg.gas_premium);
    let gas_fee_cap = to_value_str(msg.gas_fee_cap);

    let Signature { sig_type, bytes } = signature;
    let sig_encoded = base64::engine::general_purpose::STANDARD.encode(bytes);

    let params_encoded = base64::engine::general_purpose::STANDARD.encode(msg.params);
    // refer to: https://lotus.filecoin.io/reference/lotus/mpool/#mpoolpush
    json!([
        {
            "Message": {
                "Version": msg.version.unwrap_or(0),
                "To": msg.to.to_string(),
                "From": msg.from.to_string(),
                "Value": msg.value.atto().to_string(),
                "Method": msg.method,
                "Params": params_encoded,

                // THESE ALL WILL AUTO POPULATE if null
                "Nonce": nonce,
                "GasLimit": gas_limit,
                "GasFeeCap": gas_fee_cap,
                "GasPremium": gas_premium,
                "CID": CIDMap::from(msg.cid),
            },
            "Signature": {
                "Type": sig_type as u8,
                "Data": sig_encoded,
            }
        }
    ])
}
