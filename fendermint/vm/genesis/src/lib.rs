// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! A Genesis data structure similar to [genesis.Template](https://github.com/filecoin-project/lotus/blob/v1.20.4/genesis/types.go)
//! in Lotus, which is used to [initialize](https://github.com/filecoin-project/lotus/blob/v1.20.4/chain/gen/genesis/genesis.go) the state tree.

use anyhow::anyhow;
use fvm_shared::bigint::{BigInt, Integer};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use fendermint_actor_eam::PermissionModeParams;
use fvm_shared::version::NetworkVersion;
use fvm_shared::{address::Address, econ::TokenAmount};

use fendermint_crypto::{normalize_public_key, PublicKey};
use fendermint_vm_core::Timestamp;
use fendermint_vm_encoding::IsHumanReadable;

#[cfg(feature = "arb")]
mod arb;

/// Power conversion decimal points, e.g. 3 decimals means 1 power per milliFIL.
pub type PowerScale = i8;

/// The genesis data structure we serialize to JSON and start the chain with.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Genesis {
    /// The name of the blockchain.
    ///
    /// It will be used to derive a chain ID as well as being
    /// the network name in the `InitActor`.
    pub chain_name: String,
    pub timestamp: Timestamp,
    pub network_version: NetworkVersion,
    #[serde_as(as = "IsHumanReadable")]
    pub base_fee: TokenAmount,
    /// Collateral to power conversion.
    pub power_scale: PowerScale,
    /// Validators in genesis are given with their FIL collateral to maintain the
    /// highest possible fidelity when we are deriving a genesis file in IPC,
    /// where the parent subnet tracks collateral.
    pub validators: Vec<Validator<Collateral>>,
    pub accounts: Vec<Actor>,
    /// The custom eam permission mode that controls who can deploy contracts
    pub eam_permission_mode: PermissionMode,
    /// IPC related configuration, if enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipc: Option<ipc::IpcParams>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum PermissionMode {
    /// No restriction, everyone can deploy
    Unrestricted,
    /// Only whitelisted addresses can deploy
    AllowList { addresses: Vec<SignerAddr> },
}

/// Wrapper around [`Address`] to provide human readable serialization in JSON format.
///
/// An alternative would be the `serde_with` crate.
///
/// TODO: This is based on [Lotus](https://github.com/filecoin-project/lotus/blob/v1.20.4/genesis/types.go).
///       Not sure if anything but public key addresses make sense here. Consider using `PublicKey` instead of `Address`.
#[serde_as]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignerAddr(#[serde_as(as = "IsHumanReadable")] pub Address);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub owner: SignerAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Multisig {
    pub signers: Vec<SignerAddr>,
    pub threshold: u64,
    pub vesting_duration: u64,
    pub vesting_start: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActorMeta {
    Account(Account),
    Multisig(Multisig),
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Actor {
    pub meta: ActorMeta,
    #[serde_as(as = "IsHumanReadable")]
    pub balance: TokenAmount,
}

/// Total amount of tokens delegated to a validator.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Collateral(#[serde_as(as = "IsHumanReadable")] pub TokenAmount);

/// Total voting power of a validator.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub struct Power(pub u64);

impl Collateral {
    /// Convert from [Collateral] to [Power] by specifying the number of significant
    /// decimal places per FIL that grant 1 power.
    ///
    /// For example:
    /// * with 3 decimal places, we get 1 power per milli FIL: 0.001 FIL => 1 power
    /// * with 0 decimal places, we get 1 power per whole FIL: 1 FIL => 1 power
    pub fn into_power(self: Collateral, scale: PowerScale) -> Power {
        let atto_per_power = Self::atto_per_power(scale);
        let atto = self.0.atto();
        // Rounding away from zero, so with little collateral (e.g. in testing)
        // we don't end up with everyone having 0 power and then being unable
        // to produce a checkpoint because the threshold is 0.
        let power = atto.div_ceil(&atto_per_power);
        let power = power.min(BigInt::from(u64::MAX));
        Power(power.try_into().expect("clipped to u64::MAX"))
    }

    /// Helper function to convert atto to [Power].
    fn atto_per_power(scale: PowerScale) -> BigInt {
        // Figure out how many decimals we need to shift to the right.
        let decimals = match scale {
            d if d >= 0 => TokenAmount::DECIMALS.saturating_sub(d as usize) as u32,
            d => (TokenAmount::DECIMALS as i8 + d.abs()) as u32,
        };
        BigInt::from(10).pow(decimals)
    }
}

impl Default for Collateral {
    fn default() -> Self {
        Self(TokenAmount::from_atto(0))
    }
}

/// Secp256k1 public key of the validators.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatorKey(pub PublicKey);

impl ValidatorKey {
    /// Create a new key and make sure the wrapped public key is normalized,
    /// which is to ensure the results look the same after a serialization roundtrip.
    pub fn new(key: PublicKey) -> Self {
        Self(normalize_public_key(key))
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.0
    }
}

impl TryFrom<ValidatorKey> for tendermint::PublicKey {
    type Error = anyhow::Error;

    fn try_from(value: ValidatorKey) -> Result<Self, Self::Error> {
        let bz = value.0.serialize();

        let key = tendermint::crypto::default::ecdsa_secp256k1::VerifyingKey::from_sec1_bytes(&bz)
            .map_err(|e| anyhow!("failed to convert public key: {e}"))?;

        Ok(tendermint::public_key::PublicKey::Secp256k1(key))
    }
}

impl TryFrom<tendermint::PublicKey> for ValidatorKey {
    type Error = anyhow::Error;

    fn try_from(value: tendermint::PublicKey) -> Result<Self, Self::Error> {
        match value {
            tendermint::PublicKey::Secp256k1(key) => {
                let bz = key.to_sec1_bytes();
                let pk = PublicKey::parse_slice(&bz, None)?;
                Ok(Self(pk))
            }
            other => Err(anyhow!("unexpected validator key type: {other:?}")),
        }
    }
}

/// A genesis validator with their initial power.
///
/// An [`Address`] would be enough to validate signatures, however
/// we will always need the public key to return updates in the
/// power distribution to Tendermint; it is easiest to ask for
/// the full public key.
///
/// Note that we could get the validators from `InitChain` through
/// the ABCI, but then we'd have to handle the case of a key we
/// don't know how to turn into an [`Address`]. This way leaves
/// less room for error, and we can pass all the data to the FVM
/// in one go.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Validator<P> {
    pub public_key: ValidatorKey,
    pub power: P,
}

impl<A> Validator<A> {
    /// Convert the power.
    pub fn map_power<F: FnOnce(A) -> B, B>(self, f: F) -> Validator<B> {
        Validator {
            public_key: self.public_key,
            power: f(self.power),
        }
    }
}

impl From<PermissionMode> for PermissionModeParams {
    fn from(value: PermissionMode) -> Self {
        match value {
            PermissionMode::Unrestricted => PermissionModeParams::Unrestricted,
            PermissionMode::AllowList { addresses } => {
                let addresses = addresses.into_iter().map(|v| v.0).collect::<Vec<_>>();
                PermissionModeParams::AllowList(addresses)
            }
        }
    }
}

/// IPC related data structures.
pub mod ipc {
    use fendermint_vm_encoding::IsHumanReadable;
    use ipc_api::subnet_id::SubnetID;
    use serde::{Deserialize, Serialize};
    use serde_with::serde_as;

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    pub struct IpcParams {
        pub gateway: GatewayParams,
    }

    #[serde_as]
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    pub struct GatewayParams {
        #[serde_as(as = "IsHumanReadable")]
        pub subnet_id: SubnetID,
        pub bottom_up_check_period: u64,
        pub majority_percentage: u8,
        pub active_validators_limit: u16,
    }
}

#[cfg(test)]
mod tests {
    use fvm_shared::{bigint::BigInt, econ::TokenAmount};
    use num_traits::Num;
    use quickcheck_macros::quickcheck;

    use crate::{Collateral, Genesis};

    #[quickcheck]
    fn genesis_json(value0: Genesis) {
        let repr = serde_json::to_string(&value0).expect("failed to encode");
        let value1: Genesis = serde_json::from_str(&repr)
            .map_err(|e| format!("{e}; {repr}"))
            .expect("failed to decode JSON");

        assert_eq!(value1, value0)
    }

    #[quickcheck]
    fn genesis_cbor(value0: Genesis) {
        let repr = fvm_ipld_encoding::to_vec(&value0).expect("failed to encode");
        let value1: Genesis = fvm_ipld_encoding::from_slice(&repr).expect("failed to decode");

        assert_eq!(value1, value0)
    }

    #[test]
    fn tokens_to_power() {
        // Collateral given in atto (18 digits after the decimal)
        // Instead of truncating, the remainder is rounded up, to avoid giving 0 power.
        let examples: Vec<(&str, u64)> = vec![
            ("0.000000000000000000", 0),
            ("0.000000000000000001", 1),
            ("0.000999999999999999", 1),
            ("0.001000000000000000", 1),
            ("0.001999999999999999", 2),
            ("1.000000000000000000", 1000),
            ("0.999999999999999999", 1000),
            ("1.998000000000000001", 1999),
            ("1.999000000000000000", 1999),
            ("1.999000000000000001", 2000),
            ("1.999999999999999999", 2000),
            ("2.999999999999999999", 3000),
        ];

        for (atto, expected) in examples {
            let atto = BigInt::from_str_radix(atto.replace('.', "").as_str(), 10).unwrap();
            let collateral = Collateral(TokenAmount::from_atto(atto.clone()));
            let power = collateral.into_power(3).0;
            assert_eq!(power, expected, "{atto:?} atto => {power} power");
        }
    }

    #[test]
    fn atto_per_power() {
        // Collateral given in atto (18 digits after the decimal)
        let examples = vec![
            (0, TokenAmount::PRECISION),
            (3, 1_000_000_000_000_000),
            (-1, 10_000_000_000_000_000_000),
        ];

        for (scale, atto) in examples {
            assert_eq!(Collateral::atto_per_power(scale), BigInt::from(atto))
        }
    }
}
