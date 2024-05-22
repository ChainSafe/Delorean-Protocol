// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use ethers::types::U256;
use fvm_shared::{
    bigint::{BigInt, Integer, Sign, MAX_BIGINT_SIZE},
    econ::TokenAmount,
};
use lazy_static::lazy_static;
use quickcheck::Gen;
use std::str::FromStr;

lazy_static! {
    /// The max below is taken from https://github.com/filecoin-project/ref-fvm/blob/fvm%40v3.0.0-alpha.24/shared/src/bigint/bigint_ser.rs#L80-L81
    static ref MAX_BIGINT: BigInt =
        BigInt::new(Sign::Plus, vec![u32::MAX; MAX_BIGINT_SIZE / 4 - 1]);

    static ref MAX_U256: BigInt = BigInt::from_str(&U256::MAX.to_string()).unwrap();

    /// `fvm_shared::sys::TokenAmount` is limited to `u128` range.
    static ref MAX_U128: BigInt = BigInt::from(u128::MAX);

    // Restrict maximum token value to what we can actually pass to Ethereum.
    static ref MAX_ATTO: BigInt = MAX_BIGINT.clone().min(MAX_U128.clone());
}

#[derive(Clone, Debug)]
/// Unfortunately an arbitrary `TokenAmount` is not serializable if it has more than 128 bytes, we get "BigInt too large" error.
pub struct ArbTokenAmount(pub TokenAmount);

impl quickcheck::Arbitrary for ArbTokenAmount {
    fn arbitrary(g: &mut Gen) -> Self {
        let tokens = TokenAmount::arbitrary(g);
        let atto = tokens.atto();
        let atto = atto.mod_floor(&MAX_ATTO);
        Self(TokenAmount::from_atto(atto))
    }
}

impl arbitrary::Arbitrary<'_> for ArbTokenAmount {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        // Using double because the way it's generated is base don vectors,
        // and they are often empty when the `size` parameter is small.
        let atto = BigInt::arbitrary(u)? + BigInt::arbitrary(u)?;
        let atto = atto.mod_floor(&MAX_ATTO);
        Ok(Self(TokenAmount::from_atto(atto)))
    }
}
