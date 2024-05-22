// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{fmt::Display, path::Path};

use anyhow::Context;
use bollard::Docker;

use crate::{
    docker::{
        runner::{split_cmd, DockerRunner},
        user_id, FENDERMINT_IMAGE,
    },
    manifest::EnvMap,
    materials::{DefaultAccount, DefaultSubnet},
    RelayerName, ResourceHash, TestnetResource,
};

use super::{container::DockerContainer, dropper::DropChute, network::NetworkName, DropPolicy};

pub struct DockerRelayer {
    relayer_name: RelayerName,
    relayer: DockerContainer,
}

impl Display for DockerRelayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.relayer_name, f)
    }
}

impl DockerRelayer {
    /// Get or create the relayer container.
    ///
    /// This assumes that the submitter and the involved parent and child
    /// subnets have been added to the `ipc-cli` config.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_or_create<'a>(
        root: impl AsRef<Path>,
        docker: Docker,
        dropper: DropChute,
        drop_policy: &DropPolicy,
        relayer_name: &RelayerName,
        subnet: &DefaultSubnet,
        submitter: &DefaultAccount,
        network_name: Option<NetworkName>,
        env: &EnvMap,
    ) -> anyhow::Result<Self> {
        let container_name = container_name(relayer_name);

        // If the container exists, return it.
        if let Some(relayer) = DockerContainer::get(
            docker.clone(),
            dropper.clone(),
            drop_policy,
            container_name.clone(),
        )
        .await?
        {
            return Ok(Self {
                relayer_name: relayer_name.clone(),
                relayer,
            });
        }

        // We'll need to mount the IPC configuration for the relayer.
        let ipc_dir = root.as_ref().join(subnet.name.testnet()).join("ipc");

        let user = user_id(&ipc_dir)?;

        // The CLI only logs to the output. Its log level can be configured with the general env vars.
        let volumes = vec![(ipc_dir, "/fendermint/.ipc")];

        let creator = DockerRunner::new(
            docker,
            dropper,
            drop_policy.clone(),
            relayer_name.clone(),
            user,
            FENDERMINT_IMAGE,
            volumes,
            network_name,
        )
        .with_env(env.clone());

        // TODO: Do we need to use any env vars with the relayer?
        let entrypoint = split_cmd(&format!(
            "ipc-cli \
                --config-path /fendermint/.ipc/config.toml \
                checkpoint relayer \
                    --subnet {} \
                    --submitter {:?} \
            ",
            subnet.subnet_id,
            submitter.eth_addr()
        ));

        let relayer = creator
            .create(container_name, Default::default(), entrypoint)
            .await
            .context("failed to create relayer")?;

        Ok(Self {
            relayer_name: relayer_name.clone(),
            relayer,
        })
    }

    /// Start the relayer, unless it's already running.
    pub async fn start(&self) -> anyhow::Result<()> {
        self.relayer.start().await
    }
}

/// Create a container name from the relayer name.
///
/// It consists of `{relayer-id}-relayer-{hash(relayer-name)}`
fn container_name(relayer_name: &RelayerName) -> String {
    let relayer_id = relayer_name
        .path()
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let hash = ResourceHash::digest(relayer_name.path_string());
    let hash = hash.to_string();
    let hash = &hash.as_str()[..6];
    format!("{relayer_id}-relayer-{}", hash)
}
