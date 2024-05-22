// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use std::str::FromStr;

use fvm_shared::address::Address;
use indoc::formatdoc;
use ipc_api::subnet_id::SubnetID;
use ipc_types::EthAddress;
use url::Url;

use crate::config::Config;

// Arguments for the config's fields
const REPO_PATH: &str = "~/.ipc";
const CHILD_ID: &str = "/r123/f0100";
const CHILD_AUTH_TOKEN: &str = "CHILD_AUTH_TOKEN";
const PROVIDER_HTTP: &str = "http://127.0.0.1:3030/rpc/v1";
const ETH_ADDRESS: &str = "0x6be1ccf648c74800380d0520d797a170c808b624";

#[test]
fn check_keystore_config() {
    let config = read_config();
    assert_eq!(
        config.keystore_path,
        Some(REPO_PATH.to_string()),
        "invalid provider keystore path"
    );
}

#[test]
fn check_subnets_config() {
    let config = read_config().subnets;

    let child_id = SubnetID::from_str(CHILD_ID).unwrap();
    let child = &config[&child_id];
    assert_eq!(child.id, child_id);
    assert_eq!(
        child.gateway_addr(),
        Address::from(EthAddress::from_str(ETH_ADDRESS).unwrap())
    );
    assert_eq!(*child.rpc_http(), Url::from_str(PROVIDER_HTTP).unwrap(),);
    assert_eq!(child.auth_token().as_ref().unwrap(), CHILD_AUTH_TOKEN);
}

fn config_str() -> String {
    formatdoc!(
        r#"
        keystore_path = "{REPO_PATH}"

        [[subnets]]
        id = "{CHILD_ID}"

        [subnets.config]
        network_type = "fevm"
        auth_token = "{CHILD_AUTH_TOKEN}"
        provider_http = "{PROVIDER_HTTP}"
        registry_addr = "{ETH_ADDRESS}"
        gateway_addr = "{ETH_ADDRESS}"
        "#
    )
}

fn read_config() -> Config {
    Config::from_toml_str(config_str().as_str()).unwrap()
}
