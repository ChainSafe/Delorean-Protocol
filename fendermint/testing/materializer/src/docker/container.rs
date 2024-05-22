// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, Context};
use futures::StreamExt;
use std::collections::HashMap;

use bollard::{
    container::{ListContainersOptions, LogsOptions},
    secret::{ContainerInspectResponse, ContainerStateStatusEnum},
    service::ContainerSummary,
    Docker,
};

use super::{
    dropper::{DropChute, DropCommand, DropPolicy},
    DockerConstruct,
};

/// Time to wait before killing the container if it doesn't want to stop.
const KILL_TIMEOUT_SECS: i64 = 5;

pub struct DockerContainer {
    docker: Docker,
    dropper: DropChute,
    container: DockerConstruct,
}

impl DockerContainer {
    pub fn new(docker: Docker, dropper: DropChute, container: DockerConstruct) -> Self {
        Self {
            docker,
            dropper,
            container,
        }
    }

    pub fn hostname(&self) -> &str {
        &self.container.name
    }

    /// Get a container by name, if it exists.
    pub async fn get(
        docker: Docker,
        dropper: DropChute,
        drop_policy: &DropPolicy,
        name: String,
    ) -> anyhow::Result<Option<Self>> {
        let mut filters = HashMap::new();
        filters.insert("name".to_string(), vec![name.clone()]);

        let containers: Vec<ContainerSummary> = docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await
            .context("failed to list docker containers")?;

        match containers.first() {
            None => Ok(None),
            Some(container) => {
                let id = container
                    .id
                    .clone()
                    .ok_or_else(|| anyhow!("docker container {name} has no id"))?;

                Ok(Some(Self::new(
                    docker,
                    dropper,
                    DockerConstruct {
                        id,
                        name,
                        keep: drop_policy.keep(false),
                    },
                )))
            }
        }
    }

    /// Start the container, unless it's already running.
    pub async fn start(&self) -> anyhow::Result<()> {
        let inspect: ContainerInspectResponse = self
            .docker
            .inspect_container(&self.container.id, None)
            .await
            .with_context(|| {
                format!(
                    "failed to inspect container: {} ({})",
                    self.container.name, self.container.id,
                )
            })?;

        // Idempotency; we could be re-running the materializer after it failed somewhere along testnet creation.
        if let Some(ContainerStateStatusEnum::RUNNING) = inspect.state.and_then(|s| s.status) {
            return Ok(());
        }

        eprintln!(
            "STARTING CONTAINER: {} ({})",
            self.container.name, self.container.id
        );

        self.docker
            .start_container::<&str>(&self.container.id, None)
            .await
            .with_context(|| {
                format!(
                    "failed to start container: {} ({})",
                    self.container.name, self.container.id
                )
            })?;

        Ok(())
    }

    /// Simplistic way of collecting logs of containers used in the test,
    /// mostly to debug build failures on CI.
    pub async fn logs(&self) -> Vec<String> {
        let mut log_stream = self.docker.logs::<&str>(
            &self.container.name,
            Some(LogsOptions {
                stdout: true,
                stderr: true,
                follow: false,
                ..Default::default()
            }),
        );

        let mut out = Vec::new();
        while let Some(Ok(log)) = log_stream.next().await {
            out.push(log.to_string().trim().to_string());
        }
        out
    }
}

impl Drop for DockerContainer {
    fn drop(&mut self) {
        if self.container.keep {
            return;
        }
        if self
            .dropper
            .send(DropCommand::DropContainer(self.container.name.clone()))
            .is_err()
        {
            tracing::error!(
                container_name = self.container.name,
                "dropper no longer listening"
            );
        }
    }
}
