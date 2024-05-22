// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! Helper methods to convert between Ethereum and FVM data formats.

use ethers_core::types as et;
use ethers_core::types::transaction::eip2718::TypedTransaction;

pub use fendermint_vm_message::conv::from_eth::*;
use fvm_shared::{error::ExitCode, message::Message};

use crate::{error, JsonRpcResult};

pub fn to_fvm_message(tx: TypedTransaction, accept_legacy: bool) -> JsonRpcResult<Message> {
    match tx {
        TypedTransaction::Eip1559(ref tx) => {
            Ok(fendermint_vm_message::conv::from_eth::to_fvm_message(tx)?)
        }
        TypedTransaction::Legacy(_) if accept_legacy => {
            // legacy transactions are only accepted for gas estimation purposes
            // (when accept_legacy is explicitly set)
            // eth_sendRawTransaction should fail for legacy transactions.
            // For this purpose it os OK to not set `max_fee_per_gas` and
            // `max_priority_fee_per_gas`. Legacy transactions don't include
            // that information
            Ok(fendermint_vm_message::conv::from_eth::to_fvm_message(
                &tx.into(),
            )?)
        }
        TypedTransaction::Legacy(_) | TypedTransaction::Eip2930(_) => error(
            ExitCode::USR_ILLEGAL_ARGUMENT,
            "unexpected transaction type",
        ),
    }
}

/// Turn a request into the DTO returned by the API.
pub fn to_eth_transaction(
    tx: et::Eip1559TransactionRequest,
    sig: et::Signature,
    hash: et::TxHash,
) -> et::Transaction {
    et::Transaction {
        hash,
        nonce: tx.nonce.unwrap_or_default(),
        block_hash: None,
        block_number: None,
        transaction_index: None,
        from: tx.from.unwrap_or_default(),
        to: tx.to.and_then(|to| to.as_address().cloned()),
        value: tx.value.unwrap_or_default(),
        gas: tx.gas.unwrap_or_default(),
        max_fee_per_gas: tx.max_fee_per_gas,
        max_priority_fee_per_gas: tx.max_priority_fee_per_gas,
        // Strictly speaking a "Type 2" transaction should not need to set this, but we do because Blockscout
        // has a database constraint that if a transaction is included in a block this can't be null.
        gas_price: Some(
            tx.max_fee_per_gas.unwrap_or_default()
                + tx.max_priority_fee_per_gas.unwrap_or_default(),
        ),
        input: tx.data.unwrap_or_default(),
        chain_id: tx.chain_id.map(|x| et::U256::from(x.as_u64())),
        v: et::U64::from(sig.v),
        r: sig.r,
        s: sig.s,
        transaction_type: Some(2u64.into()),
        access_list: Some(tx.access_list),
        other: Default::default(),
    }
}
