// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use std::collections::HashMap;
use std::fmt::Debug;

use anyhow::Result;
use async_trait::async_trait;
use cid::Cid;
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use serde::de::DeserializeOwned;

use crate::lotus::message::chain::GetTipSetByHeightResponse;
use message::chain::ChainHeadResponse;
use message::mpool::{MpoolPushMessage, MpoolPushMessageResponseInner};
use message::state::{ReadStateResponse, StateWaitMsgResponse};
use message::wallet::{WalletKeyType, WalletListResponse};

pub mod client;
pub mod message;

/// The network version of lotus network.
/// see https://github.com/filecoin-project/go-state-types/blob/f6fd668a32b4b4a0bc39fd69d8a5f8fb11f49461/network/version.go#L7
pub type NetworkVersion = u32;

/// The Lotus client api to interact with the Lotus node.
#[async_trait]
pub trait LotusClient {
    /// Push the message to memory pool, see: https://lotus.filecoin.io/reference/lotus/mpool/#mpoolpushmessage
    async fn mpool_push_message(
        &self,
        msg: MpoolPushMessage,
    ) -> Result<MpoolPushMessageResponseInner>;

    /// Push the unsigned message to memory pool. This will ask the local key store to sign the message.
    /// In this case, make sure `from` is actually present in the local key store.
    /// See: https://lotus.filecoin.io/reference/lotus/mpool/#mpoolpush
    async fn mpool_push(&self, mut msg: MpoolPushMessage) -> Result<Cid>;

    /// Wait for the message cid of a particular nonce, see: https://lotus.filecoin.io/reference/lotus/state/#statewaitmsg
    async fn state_wait_msg(&self, cid: Cid) -> Result<StateWaitMsgResponse>;

    /// Returns the name of the network the node is synced to, see https://lotus.filecoin.io/reference/lotus/state/#statenetworkname
    async fn state_network_name(&self) -> Result<String>;

    /// Returns the network version at the given tipset, see https://lotus.filecoin.io/reference/lotus/state/#statenetworkversion
    async fn state_network_version(&self, tip_sets: Vec<Cid>) -> Result<NetworkVersion>;

    /// Returns the CID of the builtin actors manifest for the given network version, see https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-v1-unstable-methods.md#stateactormanifestcid
    async fn state_actor_code_cids(
        &self,
        network_version: NetworkVersion,
    ) -> Result<HashMap<String, Cid>>;

    /// Get the default wallet of the node, see: https://lotus.filecoin.io/reference/lotus/wallet/#walletdefaultaddress
    async fn wallet_default(&self) -> Result<Address>;

    /// List the wallets in the node, see: https://lotus.filecoin.io/reference/lotus/wallet/#walletlist
    async fn wallet_list(&self) -> Result<WalletListResponse>;

    /// Create a new wallet, see: https://lotus.filecoin.io/reference/lotus/wallet/#walletnew
    async fn wallet_new(&self, key_type: WalletKeyType) -> Result<String>;

    /// Get the balance of an address
    async fn wallet_balance(&self, address: &Address) -> Result<TokenAmount>;

    /// Read the state of the address at tipset, see: https://lotus.filecoin.io/reference/lotus/state/#statereadstate
    async fn read_state<State: DeserializeOwned + Debug>(
        &self,
        address: Address,
        tipset: Cid,
    ) -> Result<ReadStateResponse<State>>;

    /// Returns the current head of the chain.
    /// See: https://lotus.filecoin.io/reference/lotus/chain/#chainhead
    async fn chain_head(&self) -> Result<ChainHeadResponse>;

    /// Returns the heaviest epoch for the chain
    async fn current_epoch(&self) -> Result<ChainEpoch>;

    /// GetTipsetByHeight from the underlying chain
    async fn get_tipset_by_height(
        &self,
        epoch: ChainEpoch,
        tip_set: Cid,
    ) -> Result<GetTipSetByHeightResponse>;
}
