// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use bytes::Bytes;
use cid::Cid;
use num_traits::{FromPrimitive, Num};

use fendermint_vm_genesis::SignerAddr;
use fvm_shared::{
    address::{set_current_network, Address, Network},
    bigint::BigInt,
    econ::TokenAmount,
    version::NetworkVersion,
};

/// Decimals for filecoin in nano
const FIL_AMOUNT_NANO_DIGITS: u32 = 9;

pub fn parse_network_version(s: &str) -> Result<NetworkVersion, String> {
    let nv: u32 = s
        .parse()
        .map_err(|_| format!("`{s}` isn't a network version"))?;
    if nv >= 21 {
        Ok(NetworkVersion::from(nv))
    } else {
        Err("the minimum network version is 21".to_owned())
    }
}

pub fn parse_token_amount(s: &str) -> Result<TokenAmount, String> {
    BigInt::from_str_radix(s, 10)
        .map_err(|e| format!("not a token amount: {e}"))
        .map(TokenAmount::from_atto)
}

pub fn parse_full_fil(s: &str) -> Result<TokenAmount, String> {
    let f: Result<f64, _> = s.parse();
    if f.is_err() {
        return Err("input not a token amount".to_owned());
    }

    let nano = f64::trunc(f.unwrap() * (10u64.pow(FIL_AMOUNT_NANO_DIGITS) as f64));
    Ok(TokenAmount::from_nano(nano as u128))
}

pub fn parse_cid(s: &str) -> Result<Cid, String> {
    Cid::from_str(s).map_err(|e| format!("error parsing CID: {e}"))
}

pub fn parse_address(s: &str) -> Result<Address, String> {
    match s.chars().next() {
        Some('f') => set_current_network(Network::Mainnet),
        Some('t') => set_current_network(Network::Testnet),
        _ => (),
    }
    Address::from_str(s).map_err(|e| format!("error parsing address: {e}"))
}

pub fn parse_signer_addr(s: &str) -> Result<SignerAddr, String> {
    Address::from_str(s)
        .map(SignerAddr)
        .map_err(|e| format!("error parsing addresses: {e}"))
}

pub fn parse_bytes(s: &str) -> Result<Bytes, String> {
    match hex::decode(s) {
        Ok(bz) => Ok(Bytes::from(bz)),
        Err(e) => Err(format!("error parsing raw bytes as hex: {e}")),
    }
}

/// Parse a percentage value [0-100]
pub fn parse_percentage<T>(s: &str) -> Result<T, String>
where
    T: Num + FromStr + PartialOrd + TryFrom<u8>,
    <T as FromStr>::Err: std::fmt::Display,
    <T as TryFrom<u8>>::Error: std::fmt::Debug,
{
    match T::from_str(s) {
        Ok(p) if p > T::zero() && p <= T::try_from(100u8).unwrap() => Ok(p),
        Ok(_) => Err("percentage out of range".to_owned()),
        Err(e) => Err(format!("error parsing as percentage: {e}")),
    }
}

/// Parse the FVM network and set the global value.
pub fn parse_network(s: &str) -> Result<Network, String> {
    match s.to_lowercase().as_str() {
        "main" | "mainnet" | "f" => Ok(Network::Mainnet),
        "test" | "testnet" | "t" => Ok(Network::Testnet),
        n => {
            let n: u8 = n
                .parse()
                .map_err(|e| format!("expected 0 or 1 for network: {e}"))?;

            let n = Network::from_u8(n).ok_or_else(|| format!("unexpected network: {s}"))?;

            Ok(n)
        }
    }
}

pub fn parse_eth_address(s: &str) -> Result<Address, String> {
    match ipc_types::EthAddress::from_str(s) {
        Ok(a) => Ok(a.into()),
        Err(e) => Err(format!("not a valid ethereum address: {e}")),
    }
}
