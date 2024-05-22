// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Examples of passing CLI options. Some are tricky and require some values to be parsed first.
//! These examples are here so we have an easier way to test them than having to compile the app.
//!
//! ```text
//! cargo run --example network -- --help
//! cargo run --example network -- --network 1 genesis --genesis-file ./genesis.json ipc gateway --subnet-id /r123/t0456 -b 10 -t 10 -c 1.5 -f 10 -m 65
//! FVM_NETWORK=testnet cargo run --example network -- genesis --genesis-file ./genesis.json ipc gateway --subnet-id /r123/t0456 -b 10 -t 10 -c 1.5 -f 10 -m 65
//! ```

use clap::Parser;

use fendermint_app_options::{GlobalOptions, Options};

pub fn main() {
    let opts: GlobalOptions = GlobalOptions::parse();
    println!("command: {:?}", opts.cmd);

    let n = opts.global.network;
    println!("setting current network: {n:?}");
    fvm_shared::address::set_current_network(n);

    let opts: Options = Options::parse();

    println!("{opts:?}");
}
