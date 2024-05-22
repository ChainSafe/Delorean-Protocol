// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use crate::lotus::message::deserialize::{
    deserialize_address_from_str, deserialize_some_token_amount_from_num,
    deserialize_some_token_amount_from_str, deserialize_token_amount_from_str,
};
use crate::lotus::message::CIDMap;
use cid::Cid;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use fvm_shared::MethodNum;
use serde::Deserialize;
use std::str::FromStr;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct MpoolPushMessageResponse {
    pub message: MpoolPushMessageResponseInner,
    #[serde(rename = "CID")]
    pub cid: CIDMap,
}

/// The internal message payload that node rpc sends for `MpoolPushMessageResponse`.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct MpoolPushMessageResponseInner {
    to: String,
    from: String,
    pub value: String,
    pub method: MethodNum,
    pub params: String,

    pub nonce: u64,
    #[serde(rename = "GasLimit")]
    pub gas_limit: u64,
    #[serde(rename = "GasFeeCap")]
    pub gas_fee_cap: String,
    #[serde(rename = "GasPremium")]
    pub gas_premium: String,
    pub version: u16,

    #[serde(rename = "CID")]
    pub cid: CIDMap,
}

impl MpoolPushMessageResponseInner {
    pub fn cid(&self) -> anyhow::Result<Cid> {
        Cid::try_from(self.cid.clone())
    }

    pub fn to(&self) -> anyhow::Result<Address> {
        Ok(Address::from_str(&self.to)?)
    }

    pub fn from(&self) -> anyhow::Result<Address> {
        Ok(Address::from_str(&self.from)?)
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct MpoolPushMessage {
    #[serde(deserialize_with = "deserialize_address_from_str")]
    pub to: Address,
    #[serde(deserialize_with = "deserialize_address_from_str")]
    pub from: Address,
    #[serde(deserialize_with = "deserialize_token_amount_from_str")]
    pub value: TokenAmount,
    pub method: MethodNum,
    pub params: Vec<u8>,

    pub nonce: Option<u64>,
    #[serde(deserialize_with = "deserialize_some_token_amount_from_num")]
    pub gas_limit: Option<TokenAmount>,
    #[serde(deserialize_with = "deserialize_some_token_amount_from_str")]
    pub gas_fee_cap: Option<TokenAmount>,
    #[serde(deserialize_with = "deserialize_some_token_amount_from_str")]
    pub gas_premium: Option<TokenAmount>,
    pub cid: Option<Cid>,
    pub version: Option<u16>,
    pub max_fee: Option<TokenAmount>,
}

impl MpoolPushMessage {
    pub fn new(to: Address, from: Address, method: MethodNum, params: Vec<u8>) -> Self {
        MpoolPushMessage {
            to,
            from,
            method,
            params,
            value: TokenAmount::from_atto(0),
            nonce: None,
            gas_limit: None,
            gas_fee_cap: None,
            gas_premium: None,
            cid: None,
            version: None,
            max_fee: None,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct EstimateGasResponse {
    #[serde(deserialize_with = "deserialize_some_token_amount_from_num")]
    pub gas_limit: Option<TokenAmount>,
    #[serde(deserialize_with = "deserialize_some_token_amount_from_str")]
    pub gas_fee_cap: Option<TokenAmount>,
    #[serde(deserialize_with = "deserialize_some_token_amount_from_str")]
    pub gas_premium: Option<TokenAmount>,
}
