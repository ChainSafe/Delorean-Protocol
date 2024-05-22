// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use anyhow::anyhow;
use base64::Engine;
use fil_actors_runtime::cbor;
use fvm_ipld_encoding::RawBytes;
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::lotus::message::CIDMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StateWaitMsgResponse {
    #[allow(dead_code)]
    message: CIDMap,
    #[allow(dead_code)]
    pub(crate) receipt: Receipt,
    #[allow(dead_code)]
    tip_set: Vec<CIDMap>,
    pub height: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ReadStateResponse<State> {
    #[allow(dead_code)]
    pub balance: String,
    #[allow(dead_code)]
    pub code: CIDMap,
    #[allow(dead_code)]
    pub state: State,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Receipt {
    #[allow(dead_code)]
    exit_code: u32,
    #[serde(rename = "Return")]
    pub result: Option<String>,
    #[allow(dead_code)]
    gas_used: u64,
}

impl Receipt {
    pub fn parse_result_into<T: Default + DeserializeOwned>(self) -> anyhow::Result<T> {
        if self.result.is_none() {
            return Ok(Default::default());
        }

        let r = base64::engine::general_purpose::STANDARD
            .decode(self.result.unwrap())
            .map_err(|e| {
                tracing::error!("cannot base64 decode due to {e:?}");
                anyhow!("cannot decode return string")
            })?;

        cbor::deserialize::<T>(
            &RawBytes::new(r),
            "deserialize create subnet return response",
        )
        .map_err(|e| {
            tracing::error!("cannot decode bytes due to {e:?}");
            anyhow!("cannot cbor deserialize return data")
        })
    }
}
