// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_shared::{
    bigint::{BigInt, Zero},
    econ::TokenAmount,
    message::Message,
};

// Copy of https://github.com/filecoin-project/ref-fvm/blob/fvm%40v3.3.1/fvm/src/gas/outputs.rs
mod output;

// https://github.com/filecoin-project/lotus/blob/6cc506f5cf751215be6badc94a960251c6453202/node/impl/full/eth.go#L2220C41-L2228
pub fn effective_gas_price(msg: &Message, base_fee: &TokenAmount, gas_used: i64) -> TokenAmount {
    let out = output::GasOutputs::compute(
        gas_used.try_into().expect("gas should be u64 convertible"),
        msg.gas_limit,
        base_fee,
        &msg.gas_fee_cap,
        &msg.gas_premium,
    );

    let total_spend = out.base_fee_burn + out.miner_tip + out.over_estimation_burn;

    if gas_used > 0 {
        TokenAmount::from_atto(total_spend.atto() / TokenAmount::from_atto(gas_used).atto())
    } else {
        TokenAmount::from_atto(0)
    }
}

// https://github.com/filecoin-project/lotus/blob/9e4f1a4d23ad72ab191754d4f432e4dc754fce1b/chain/types/message.go#L227
pub fn effective_gas_premium(msg: &Message, base_fee: &TokenAmount) -> TokenAmount {
    let available = if msg.gas_fee_cap < *base_fee {
        TokenAmount::from_atto(0)
    } else {
        msg.gas_fee_cap.clone() - base_fee
    };
    if msg.gas_premium < available {
        return msg.gas_premium.clone();
    }
    available
}

// finds 55th percntile instead of median to put negative pressure on gas price
// Rust implementation of:
// https://github.com/consensus-shipyard/lotus/blob/156f5556b3ecc042764d76308dca357da3adfb4d/node/impl/full/gas.go#L144
pub fn median_gas_premium(prices: &mut [(TokenAmount, i64)], block_gas_target: i64) -> TokenAmount {
    // Sort in descending order based on premium
    prices.sort_by(|a, b| b.0.cmp(&a.0));
    let blocks = prices.len() as i64;

    let mut at = block_gas_target * blocks / 2;
    at += block_gas_target * blocks / (2 * 20);

    let mut prev1 = TokenAmount::zero();
    let mut prev2 = TokenAmount::zero();

    for (price, limit) in prices.iter() {
        prev2 = prev1.clone();
        prev1 = price.clone();
        at -= limit;
        if at < 0 {
            break;
        }
    }

    let mut premium = prev1;

    if prev2 != TokenAmount::zero() {
        premium += &prev2;
        premium.div_ceil(BigInt::from(2));
    }

    premium
}
