// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use fvm_shared::address::Address;
use std::str::FromStr;

use crate::lotus::message::deserialize::{
    deserialize_ipc_address_from_map, deserialize_subnet_id_from_map,
    deserialize_token_amount_from_str,
};
use crate::manager::SubnetInfo;
use fvm_shared::econ::TokenAmount;
use ipc_api::address::IPCAddress;
use ipc_api::subnet_id::SubnetID;

#[test]
fn test_ipc_address_from_map() {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct IPCAddressWrapper {
        #[allow(dead_code)]
        #[serde(rename = "From")]
        #[serde(deserialize_with = "deserialize_ipc_address_from_map")]
        from: IPCAddress,
    }

    let raw_str = r#"
    {
        "From": {
            "SubnetId": {
                "Root": 123,
                "Children": ["f064"]
            },
            "RawAddress": "f064"
        }
    }"#;

    let w: Result<IPCAddressWrapper, _> = serde_json::from_str(raw_str);
    assert!(w.is_ok());

    assert_eq!(
        w.unwrap().from,
        IPCAddress::new(
            &SubnetID::from_str("/r123/f064").unwrap(),
            &Address::from_str("f064").unwrap()
        )
        .unwrap()
    )
}

#[test]
fn test_subnet_from_map() {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct SubnetIdWrapper {
        #[allow(dead_code)]
        #[serde(rename = "ID")]
        #[serde(deserialize_with = "deserialize_subnet_id_from_map")]
        id: SubnetID,
    }

    let raw_str = r#"
    {
        "ID": {
            "Root": 123,
            "Children": ["f01", "f064"]
        }
    }"#;

    let w: Result<SubnetIdWrapper, _> = serde_json::from_str(raw_str);
    assert!(w.is_ok());
    assert_eq!(w.unwrap().id, SubnetID::from_str("/r123/f01/f064").unwrap())
}

#[test]
fn test_subnet_from_map_error() {
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct SubnetIdWrapper {
        #[allow(dead_code)]
        #[serde(rename = "ID")]
        #[serde(deserialize_with = "deserialize_subnet_id_from_map")]
        id: SubnetID,
    }

    let raw_str = r#"
    {
        "Id": {
            "Root": 65,
            "Children": "f064"
        }
    }"#;

    let w: Result<SubnetIdWrapper, _> = serde_json::from_str(raw_str);
    assert!(w.is_err());
}

#[test]
fn test_token_amount_from_str() {
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Wrapper {
        #[allow(dead_code)]
        #[serde(deserialize_with = "deserialize_token_amount_from_str")]
        token_amount: TokenAmount,
    }

    let raw_str = r#"
    {
        "TokenAmount": "20000000000000000000"
    }"#;

    let w: Result<Wrapper, _> = serde_json::from_str(raw_str);
    assert!(w.is_ok());
    assert_eq!(w.unwrap().token_amount, TokenAmount::from_whole(20));
}

#[test]
fn test_subnet_info_to_str() {
    let s = SubnetInfo {
        id: Default::default(),
        stake: Default::default(),
        circ_supply: Default::default(),
        genesis_epoch: 0,
    };

    let w = serde_json::to_string(&s);
    assert!(w.is_ok());
}
