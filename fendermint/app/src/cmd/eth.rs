// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use anyhow::Context;
use fendermint_eth_api::HybridClient;

use crate::{
    cmd,
    options::eth::{EthArgs, EthCommands},
    settings::eth::EthSettings,
};

cmd! {
  EthArgs(self, settings: EthSettings) {
    match self.command.clone() {
      EthCommands::Run { ws_url, http_url, connect_retry_delay } => {

        let (client, driver) = HybridClient::new(http_url, ws_url, Duration::from_secs(connect_retry_delay)).context("failed to create HybridClient")?;

        let driver_handle = tokio::spawn(async move { driver.run().await });

        let result = run(settings, client).await;

        // Await the driver's termination to ensure proper connection closure.
        let _ = driver_handle.await;
        result
      }
    }
  }
}

/// Run the Ethereum API facade.
async fn run(settings: EthSettings, client: HybridClient) -> anyhow::Result<()> {
    let gas = fendermint_eth_api::GasOpt {
        min_gas_premium: settings.gas.min_gas_premium,
        num_blocks_max_prio_fee: settings.gas.num_blocks_max_prio_fee,
        max_fee_hist_size: settings.gas.max_fee_hist_size,
    };
    fendermint_eth_api::listen(
        settings.listen,
        client,
        settings.filter_timeout,
        settings.cache_capacity,
        settings.max_nonce_gap,
        gas,
    )
    .await
}
