// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{collections::HashMap, fmt::Display};

use anyhow::{bail, Context};
use bollard::{
    container::{
        AttachContainerOptions, AttachContainerResults, Config, CreateContainerOptions,
        RemoveContainerOptions,
    },
    network::ConnectNetworkOptions,
    secret::{ContainerInspectResponse, HostConfig, PortBinding},
    Docker,
};
use futures::StreamExt;

use crate::{docker::current_network, manifest::EnvMap, ResourceName, TestnetResource};

use super::{
    container::DockerContainer,
    dropper::{DropChute, DropPolicy},
    network::NetworkName,
    DockerConstruct, Volumes,
};

pub struct DockerRunner<N> {
    docker: Docker,
    dropper: DropChute,
    drop_policy: DropPolicy,
    name: N,
    user: u32,
    image: String,
    volumes: Volumes,
    network_name: Option<NetworkName>,
    env: EnvMap,
}

impl<N> DockerRunner<N>
where
    N: AsRef<ResourceName> + TestnetResource + Display,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        docker: Docker,
        dropper: DropChute,
        drop_policy: DropPolicy,
        name: N,
        user: u32,
        image: &str,
        volumes: Volumes,
        network_name: Option<NetworkName>,
    ) -> Self {
        Self {
            docker,
            dropper,
            drop_policy,
            name,
            user,
            image: image.to_string(),
            volumes,
            network_name,
            env: EnvMap::default(),
        }
    }

    pub fn with_env(mut self, env: EnvMap) -> Self {
        self.env = env;
        self
    }

    // Tag containers with resource names.
    fn labels(&self) -> HashMap<String, String> {
        [
            ("testnet", self.name.testnet().path()),
            ("resource", self.name.as_ref().path()),
        ]
        .into_iter()
        .map(|(n, p)| (n.to_string(), p.to_string_lossy().to_string()))
        .collect()
    }

    fn env(&self) -> Vec<String> {
        // Set the network otherwise we might be be able to parse addresses we created.
        let network = current_network();
        let mut env = vec![
            format!("FM_NETWORK={}", network),
            format!("IPC_NETWORK={}", network),
            format!("NETWORK={}", network),
        ];
        env.extend(self.env.iter().map(|(k, v)| format!("{k}={v}")));
        env
    }

    /// Run a short lived container.
    pub async fn run_cmd(&self, cmd: &str) -> anyhow::Result<Vec<String>> {
        let cmdv = split_cmd(cmd);

        let config = Config {
            image: Some(self.image.clone()),
            user: Some(self.user.to_string()),
            cmd: Some(cmdv),
            attach_stderr: Some(true),
            attach_stdout: Some(true),
            tty: Some(true),
            labels: Some(self.labels()),
            env: Some(self.env()),
            host_config: Some(HostConfig {
                // We'll remove it explicitly at the end after collecting the output.
                auto_remove: Some(false),
                init: Some(true),
                binds: Some(
                    self.volumes
                        .iter()
                        .map(|(h, c)| format!("{}:{c}", h.to_string_lossy()))
                        .collect(),
                ),
                network_mode: self.network_name.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let id = self
            .docker
            .create_container::<&str, _>(None, config)
            .await
            .context("failed to create container")?
            .id;

        let AttachContainerResults { mut output, .. } = self
            .docker
            .attach_container::<String>(
                &id,
                Some(AttachContainerOptions {
                    stdout: Some(true),
                    stderr: Some(true),
                    stream: Some(true),
                    ..Default::default()
                }),
            )
            .await
            .context("failed to attach to container")?;

        self.docker
            .start_container::<&str>(&id, None)
            .await
            .context("failed to start container")?;

        // Collect docker attach output
        let mut out = Vec::new();
        while let Some(Ok(output)) = output.next().await {
            out.push(output.to_string());
        }

        eprintln!("RESOURCE: {} ({id})", self.name);
        eprintln!("CMD: {cmd}");
        for o in out.iter() {
            eprint!("OUT: {o}");
        }
        eprintln!("---");

        let inspect: ContainerInspectResponse = self
            .docker
            .inspect_container(&id, None)
            .await
            .context("failed to inspect container")?;

        self.docker
            .remove_container(
                &id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;

        if let Some(ref state) = inspect.state {
            let exit_code = state.exit_code.unwrap_or_default();
            if exit_code != 0 {
                bail!(
                    "container exited with code {exit_code}: '{}'",
                    state.error.clone().unwrap_or_default()
                );
            }
        }

        Ok(out)
    }

    /// Create a container to be started later.
    pub async fn create(
        &self,
        name: String,
        // Host <-> Container port mappings
        ports: Vec<(u32, u32)>,
        entrypoint: Vec<String>,
    ) -> anyhow::Result<DockerContainer> {
        let config = Config {
            hostname: Some(name.clone()),
            image: Some(self.image.clone()),
            user: Some(self.user.to_string()),
            entrypoint: Some(entrypoint),
            labels: Some(self.labels()),
            env: Some(self.env()),
            cmd: None,
            host_config: Some(HostConfig {
                init: Some(true),
                binds: Some(
                    self.volumes
                        .iter()
                        .map(|(h, c)| format!("{}:{c}", h.to_string_lossy()))
                        .collect(),
                ),
                port_bindings: Some(
                    ports
                        .into_iter()
                        .flat_map(|(h, c)| {
                            let binding = PortBinding {
                                host_ip: None,
                                host_port: Some(h.to_string()),
                            };
                            // Emitting both TCP and UDP, just in case.
                            vec![
                                (format!("{c}/tcp"), Some(vec![binding.clone()])),
                                (format!("{c}/udp"), Some(vec![binding])),
                            ]
                        })
                        .collect(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        };

        let id = self
            .docker
            .create_container::<String, _>(
                Some(CreateContainerOptions {
                    name: name.clone(),
                    ..Default::default()
                }),
                config,
            )
            .await
            .context("failed to create container")?
            .id;

        eprintln!("RESOURCE: {}", self.name);
        eprintln!("CREATED CONTAINER: {} ({})", name, id);
        eprintln!("---");

        // host_config.network_mode should work as well.
        if let Some(network_name) = self.network_name.as_ref() {
            self.docker
                .connect_network(
                    network_name,
                    ConnectNetworkOptions {
                        container: id.clone(),
                        ..Default::default()
                    },
                )
                .await
                .context("failed to connect container to network")?;
        }

        Ok(DockerContainer::new(
            self.docker.clone(),
            self.dropper.clone(),
            DockerConstruct {
                id,
                name,
                keep: self.drop_policy.keep(true),
            },
        ))
    }
}

pub fn split_cmd(cmd: &str) -> Vec<String> {
    cmd.split_ascii_whitespace()
        .map(|s| s.to_string())
        .collect()
}
