// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use crate::{CommandLineHandler, GlobalArguments};
use async_trait::async_trait;
use ipc_provider::config::DEFAULT_CONFIG_TEMPLATE;
use std::io::Write;

use clap::Args;

/// The command to initialize a new config template in a specific path
pub(crate) struct InitConfig;

#[async_trait]
impl CommandLineHandler for InitConfig {
    type Arguments = InitConfigArgs;

    async fn handle(global: &GlobalArguments, _arguments: &Self::Arguments) -> anyhow::Result<()> {
        let path = global.config_path();
        log::debug!("initializing empty config file in {}", path);

        let file_path = std::path::Path::new(&path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&path).map_err(|e| {
            log::error!("couldn't create config file");
            e
        })?;
        file.write_all(DEFAULT_CONFIG_TEMPLATE.as_bytes())
            .map_err(|e| {
                log::error!("error populating empty config template");
                e
            })?;

        log::info!("Empty config populated successful in {}", &path);

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Arguments to initialize a new empty config file")]
pub(crate) struct InitConfigArgs {}
