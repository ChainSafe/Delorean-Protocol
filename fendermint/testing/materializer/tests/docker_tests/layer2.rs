// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::{anyhow, bail, Context};
use ethers::core::types as et;
use ethers::providers::Middleware;
use futures::FutureExt;
use std::sync::Arc;
use std::time::Duration;

use fendermint_materializer::{HasEthApi, ResourceId};
use fendermint_vm_actor_interface::init::builtin_actor_eth_addr;
use fendermint_vm_actor_interface::ipc;
use fendermint_vm_message::conv::from_fvm::to_eth_address;
use ipc_actors_abis::gateway_getter_facet::{GatewayGetterFacet, ParentFinality};
use ipc_actors_abis::subnet_actor_getter_facet::SubnetActorGetterFacet;

use crate::with_testnet;

const MANIFEST: &str = "layer2.yaml";
const CHECKPOINT_PERIOD: u64 = 10;
const SLEEP_SECS: u64 = 5;
const MAX_RETRIES: u32 = 5;

/// Test that top-down syncing and bottom-up checkpoint submission work.
#[serial_test::serial]
#[tokio::test]
async fn test_topdown_and_bottomup() {
    with_testnet(
        MANIFEST,
        |manifest| {
            // Try to make sure the bottom-up checkpoint period is quick enough for reasonable test runtime.
            let subnet = manifest
                .subnets
                .get_mut(&ResourceId::from("england"))
                .expect("subnet not found");

            subnet.bottom_up_checkpoint.period = CHECKPOINT_PERIOD;
        },
        |_, _, testnet| {
            let test = async {
                let brussels = testnet.node(&testnet.root().node("brussels"))?;
                let london = testnet.node(&testnet.root().subnet("england").node("london"))?;
                let england = testnet.subnet(&testnet.root().subnet("england"))?;

                let london_provider = Arc::new(
                    london
                        .ethapi_http_provider()?
                        .ok_or_else(|| anyhow!("ethapi should be enabled"))?,
                );

                let brussels_provider = Arc::new(
                    brussels
                        .ethapi_http_provider()?
                        .ok_or_else(|| anyhow!("ethapi should be enabled"))?,
                );

                // Gateway actor on the child
                let england_gateway = GatewayGetterFacet::new(
                    builtin_actor_eth_addr(ipc::GATEWAY_ACTOR_ID),
                    london_provider.clone(),
                );

                // Subnet actor on the parent
                let england_subnet = SubnetActorGetterFacet::new(
                    to_eth_address(&england.subnet_id.subnet_actor())
                        .and_then(|a| a.ok_or_else(|| anyhow!("not an eth address")))?,
                    brussels_provider.clone(),
                );

                // Query the latest committed parent finality and compare to the parent.
                {
                    let mut retry = 0;
                    loop {
                        let finality: ParentFinality = england_gateway
                            .get_latest_parent_finality()
                            .call()
                            .await
                            .context("failed to get parent finality")?;

                        // If the latest finality is not zero it means the syncer is working,
                        if finality.height.is_zero() {
                            if retry < MAX_RETRIES {
                                eprintln!("waiting for syncing with the parent...");
                                tokio::time::sleep(Duration::from_secs(SLEEP_SECS)).await;
                                retry += 1;
                                continue;
                            }
                            bail!("the parent finality is still zero");
                        }

                        // Check that the block hash of the parent is actually the same at that height.
                        let parent_block: Option<et::Block<_>> = brussels_provider
                            .get_block(finality.height.as_u64())
                            .await
                            .context("failed to get parent block")?;

                        let Some(parent_block_hash) = parent_block.and_then(|b| b.hash) else {
                            bail!("cannot find parent block at final height");
                        };

                        if parent_block_hash.0 != finality.block_hash {
                            bail!("the finality block hash is different from the API");
                        }
                        break;
                    }
                }

                // Check that the parent knows about a checkpoint submitted from the child.
                {
                    let mut retry = 0;
                    loop {
                        // NOTE: The implementation of the following method seems like a nonsense;
                        //       I don't know if there is a way to ask the gateway what the latest
                        //       checkpoint is, so we'll just have to go to the parent directly.
                        // let (has_checkpoint, epoch, _): (bool, et::U256, _) = england_gateway
                        //     .get_current_bottom_up_checkpoint()
                        //     .call()
                        //     .await
                        //     .context("failed to get current bottomup checkpoint")?;
                        let ckpt_height: et::U256 = england_subnet
                            .last_bottom_up_checkpoint_height()
                            .call()
                            .await
                            .context("failed to query last checkpoint height")?;

                        if !ckpt_height.is_zero() {
                            break;
                        }

                        if retry < MAX_RETRIES {
                            eprintln!("waiting for a checkpoint to be submitted...");
                            tokio::time::sleep(Duration::from_secs(SLEEP_SECS)).await;
                            retry += 1;
                            continue;
                        }

                        bail!("hasn't submitted a bottom-up checkpoint");
                    }
                }

                Ok(())
            };

            test.boxed_local()
        },
    )
    .await
    .unwrap()
}
