// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_app_options::config::ConfigArgs;

use crate::{cmd, settings::Settings};

cmd! {
  ConfigArgs(self, settings) {
    print_settings(settings)
  }
}

fn print_settings(settings: Settings) -> anyhow::Result<()> {
    // Currently the `Settings` doesn't support `Serialize`,
    // but if it did we could choose a format to print in.
    println!("{settings:?}");
    Ok(())
}
