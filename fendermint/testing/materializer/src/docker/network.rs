// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{collections::HashMap, fmt::Display};

use anyhow::{anyhow, Context};
use bollard::{
    network::{CreateNetworkOptions, ListNetworksOptions},
    service::{Network, NetworkCreateResponse},
    Docker,
};

use crate::TestnetName;

use super::{
    dropper::{DropChute, DropCommand, DropPolicy},
    DockerConstruct,
};

pub type NetworkName = String;

pub struct DockerNetwork {
    docker: Docker,
    dropper: DropChute,
    /// There is a single docker network created for the entire testnet.
    testnet_name: TestnetName,
    network: DockerConstruct,
}

impl Display for DockerNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.network_name(), f)
    }
}

impl DockerNetwork {
    pub fn testnet_name(&self) -> &TestnetName {
        &self.testnet_name
    }

    pub fn network_name(&self) -> &NetworkName {
        &self.network.name
    }

    /// Check if an externally managed network already exists;
    /// if not, create a new docker network for the testnet.
    pub async fn get_or_create(
        docker: Docker,
        dropper: DropChute,
        testnet_name: TestnetName,
        drop_policy: &DropPolicy,
    ) -> anyhow::Result<Self> {
        let network_name = testnet_name.path_string();

        let mut filters = HashMap::new();
        filters.insert("name".to_string(), vec![network_name.clone()]);

        let networks: Vec<Network> = docker
            .list_networks(Some(ListNetworksOptions { filters }))
            .await
            .context("failed to list docker networks")?;

        let (id, is_new) = match networks.first() {
            None => {
                let network: NetworkCreateResponse = docker
                    .create_network(CreateNetworkOptions {
                        name: network_name.clone(),
                        ..Default::default()
                    })
                    .await
                    .context("failed to create docker network")?;

                let id = network
                    .id
                    .clone()
                    .ok_or_else(|| anyhow!("created docker network has no id"))?;

                (id, true)
            }
            Some(network) => {
                let id = network
                    .id
                    .clone()
                    .ok_or_else(|| anyhow!("docker network {network_name} has no id"))?;

                (id, false)
            }
        };

        Ok(Self {
            docker,
            dropper,
            testnet_name,
            network: DockerConstruct {
                id,
                name: network_name,
                keep: drop_policy.keep(is_new),
            },
        })
    }
}

impl Drop for DockerNetwork {
    fn drop(&mut self) {
        if self.network.keep {
            return;
        }
        if self
            .dropper
            .send(DropCommand::DropNetwork(self.network.name.clone()))
            .is_err()
        {
            tracing::error!(
                network_name = self.network.name,
                "dropper no longer listening"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use bollard::Docker;
    use std::time::Duration;

    use super::DockerNetwork;
    use crate::{
        docker::dropper::{self, DropPolicy},
        TestnetName,
    };

    #[tokio::test]
    async fn test_network() {
        let tn = TestnetName::new("test-network");

        let docker = Docker::connect_with_local_defaults().expect("failed to connect to docker");
        let (drop_handle, drop_chute) = dropper::start(docker.clone());
        let drop_policy = DropPolicy::default();

        let n1 = DockerNetwork::get_or_create(
            docker.clone(),
            drop_chute.clone(),
            tn.clone(),
            &drop_policy,
        )
        .await
        .expect("failed to create network");

        let n2 = DockerNetwork::get_or_create(docker.clone(), drop_chute, tn.clone(), &drop_policy)
            .await
            .expect("failed to get network");

        assert!(
            !n1.network.keep,
            "when created, the network should not be marked to keep"
        );
        assert!(
            n2.network.keep,
            "when already exists, the network should be kept"
        );
        assert_eq!(n1.network.id, n2.network.id);
        assert_eq!(n1.network.name, n2.network.name);
        assert_eq!(n1.network.name, "testnets/test-network");

        let id = n1.network.id.clone();

        let exists = || async {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let ns = docker.list_networks::<String>(None).await.unwrap();
            ns.iter().any(|n| n.id == Some(id.clone()))
        };

        drop(n2);
        assert!(exists().await, "network still exists after n2 dropped");

        drop(n1);

        let _ = drop_handle.await;

        assert!(
            !exists().await,
            "network should be removed when n1 is dropped"
        );
    }
}
