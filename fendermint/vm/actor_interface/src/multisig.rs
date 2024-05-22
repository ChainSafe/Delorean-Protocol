// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_hamt::Hamt;
use fvm_shared::{address::Address, clock::ChainEpoch, econ::TokenAmount, ActorID, HAMT_BIT_WIDTH};
use serde::{Deserialize, Serialize};

define_code!(MULTISIG { code_id: 9 });

/// Transaction ID type
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Hash, Eq, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct TxnID(pub i64);

/// Multisig actor state
#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone)]
pub struct State {
    pub signers: Vec<Address>,
    pub num_approvals_threshold: u64,
    pub next_tx_id: TxnID,

    // Linear unlock
    pub initial_balance: TokenAmount,
    pub start_epoch: ChainEpoch,
    pub unlock_duration: ChainEpoch,

    pub pending_txs: Cid,
}

impl State {
    pub fn new<BS: Blockstore>(
        store: &BS,
        signers: Vec<ActorID>,
        threshold: u64,
        start: ChainEpoch,
        duration: ChainEpoch,
        balance: TokenAmount,
    ) -> anyhow::Result<Self> {
        let empty_map_cid = Hamt::<_, ()>::new_with_bit_width(store, HAMT_BIT_WIDTH).flush()?;
        let state = Self {
            signers: signers.into_iter().map(Address::new_id).collect(),
            num_approvals_threshold: threshold,
            next_tx_id: Default::default(),
            initial_balance: balance,
            start_epoch: start,
            unlock_duration: duration,
            pending_txs: empty_map_cid,
        };
        Ok(state)
    }
}
