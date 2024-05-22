// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, bail, Context};
use async_stm::atomically_or_err;
use fendermint_abci::ApplicationService;
use fendermint_app::events::{ParentFinalityVoteAdded, ParentFinalityVoteIgnored};
use fendermint_app::ipc::{AppParentFinalityQuery, AppVote};
use fendermint_app::{App, AppConfig, AppStore, BitswapBlockstore};
use fendermint_app_settings::AccountKind;
use fendermint_crypto::SecretKey;
use fendermint_rocksdb::{blockstore::NamespaceBlockstore, namespaces, RocksDb, RocksDbConfig};
use fendermint_tracing::emit;
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_interpreter::chain::ChainEnv;
use fendermint_vm_interpreter::fvm::upgrades::UpgradeScheduler;
use fendermint_vm_interpreter::{
    bytes::{BytesMessageInterpreter, ProposalPrepareMode},
    chain::{ChainMessageInterpreter, CheckpointPool},
    fvm::{Broadcaster, FvmMessageInterpreter, ValidatorContext},
    signed::SignedMessageInterpreter,
};
use fendermint_vm_resolver::ipld::IpldResolver;
use fendermint_vm_snapshot::{SnapshotManager, SnapshotParams};
use fendermint_vm_topdown::proxy::IPCProviderProxy;
use fendermint_vm_topdown::sync::launch_polling_syncer;
use fendermint_vm_topdown::voting::{publish_vote_loop, Error as VoteError, VoteTally};
use fendermint_vm_topdown::{CachedFinalityProvider, IPCParentFinality, Toggle};
use fvm_shared::address::{current_network, Address, Network};
use ipc_ipld_resolver::{Event as ResolverEvent, VoteRecord};
use ipc_provider::config::subnet::{EVMSubnet, SubnetConfig};
use ipc_provider::IpcProvider;
use libp2p::identity::secp256k1;
use libp2p::identity::Keypair;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tower::ServiceBuilder;
use tracing::info;

use crate::cmd::key::read_secret_key;
use crate::{cmd, options::run::RunArgs, settings::Settings};

cmd! {
  RunArgs(self, settings) {
    run(settings).await
  }
}

// Database collection names.
namespaces! {
    Namespaces {
        app,
        state_hist,
        state_store,
        bit_store
    }
}

/// Run the Fendermint ABCI Application.
///
/// This method acts as our composition root.
async fn run(settings: Settings) -> anyhow::Result<()> {
    let tendermint_rpc_url = settings.tendermint_rpc_url()?;
    tracing::info!("Connecting to Tendermint at {tendermint_rpc_url}");

    let tendermint_client: tendermint_rpc::HttpClient =
        tendermint_rpc::HttpClient::new(tendermint_rpc_url)
            .context("failed to create Tendermint client")?;

    // Prometheus metrics
    let metrics_registry = if settings.metrics.enabled {
        let registry = prometheus::Registry::new();

        fendermint_app::metrics::register_app_metrics(&registry)
            .context("failed to register metrics")?;

        Some(registry)
    } else {
        None
    };

    let validator = match settings.validator_key {
        Some(ref key) => {
            let sk = key.path(settings.home_dir());
            if sk.exists() && sk.is_file() {
                let sk = read_secret_key(&sk).context("failed to read validator key")?;
                let addr = to_address(&sk, &key.kind)?;
                tracing::info!("validator key address: {addr} detected");
                Some((sk, addr))
            } else {
                bail!("validator key does not exist: {}", sk.to_string_lossy());
            }
        }
        None => {
            tracing::debug!("validator key not configured");
            None
        }
    };

    let validator_keypair = validator.as_ref().map(|(sk, _)| {
        let mut bz = sk.serialize();
        let sk = libp2p::identity::secp256k1::SecretKey::try_from_bytes(&mut bz)
            .expect("secp256k1 secret key");
        let kp = libp2p::identity::secp256k1::Keypair::from(sk);
        libp2p::identity::Keypair::from(kp)
    });

    let validator_ctx = validator.map(|(sk, addr)| {
        // For now we are using the validator key for submitting transactions.
        // This allows us to identify transactions coming from empowered validators, to give priority to protocol related transactions.
        let broadcaster = Broadcaster::new(
            tendermint_client.clone(),
            addr,
            sk.clone(),
            settings.fvm.gas_fee_cap.clone(),
            settings.fvm.gas_premium.clone(),
            settings.fvm.gas_overestimation_rate,
        )
        .with_max_retries(settings.broadcast.max_retries)
        .with_retry_delay(settings.broadcast.retry_delay);

        ValidatorContext::new(sk, broadcaster)
    });

    let testing_settings = match settings.testing.as_ref() {
        Some(_) if current_network() == Network::Mainnet => {
            bail!("testing settings are not allowed on Mainnet");
        }
        other => other,
    };

    let interpreter = FvmMessageInterpreter::<NamespaceBlockstore, _>::new(
        tendermint_client.clone(),
        validator_ctx,
        settings.contracts_dir(),
        settings.fvm.gas_overestimation_rate,
        settings.fvm.gas_search_step,
        settings.fvm.exec_in_check,
        UpgradeScheduler::new(),
    )
    .with_push_chain_meta(testing_settings.map_or(true, |t| t.push_chain_meta));

    let interpreter = SignedMessageInterpreter::new(interpreter);
    let interpreter = ChainMessageInterpreter::<_, NamespaceBlockstore>::new(interpreter);
    let interpreter = BytesMessageInterpreter::new(
        interpreter,
        ProposalPrepareMode::PrependOnly,
        false,
        settings.abci.block_max_msgs,
    );

    let ns = Namespaces::default();
    let db = open_db(&settings, &ns).context("error opening DB")?;

    // Blockstore for actors.
    let state_store =
        NamespaceBlockstore::new(db.clone(), ns.state_store).context("error creating state DB")?;

    let checkpoint_pool = CheckpointPool::new();
    let parent_finality_votes = VoteTally::empty();

    let topdown_enabled = settings.topdown_enabled();

    // If enabled, start a resolver that communicates with the application through the resolve pool.
    if settings.resolver_enabled() {
        let mut service =
            make_resolver_service(&settings, db.clone(), state_store.clone(), ns.bit_store)?;

        // Register all metrics from the IPLD resolver stack
        if let Some(ref registry) = metrics_registry {
            service
                .register_metrics(registry)
                .context("failed to register IPLD resolver metrics")?;
        }

        let client = service.client();

        let own_subnet_id = settings.ipc.subnet_id.clone();

        client
            .add_provided_subnet(own_subnet_id.clone())
            .context("error adding own provided subnet.")?;

        let resolver = IpldResolver::new(
            client.clone(),
            checkpoint_pool.queue(),
            settings.resolver.retry_delay,
            own_subnet_id.clone(),
        );

        if topdown_enabled {
            if let Some(key) = validator_keypair {
                let parent_finality_votes = parent_finality_votes.clone();

                tracing::info!("starting the parent finality vote gossip loop...");
                tokio::spawn(async move {
                    publish_vote_loop(
                        parent_finality_votes,
                        settings.ipc.vote_interval,
                        settings.ipc.vote_timeout,
                        key,
                        own_subnet_id,
                        client,
                        |height, block_hash| {
                            AppVote::ParentFinality(IPCParentFinality { height, block_hash })
                        },
                    )
                    .await
                });
            }
        } else {
            tracing::info!("parent finality vote gossip disabled");
        }

        tracing::info!("subscribing to gossip...");
        let rx = service.subscribe();
        let parent_finality_votes = parent_finality_votes.clone();
        tokio::spawn(async move {
            dispatch_resolver_events(rx, parent_finality_votes, topdown_enabled).await;
        });

        tracing::info!("starting the IPLD Resolver Service...");
        tokio::spawn(async move {
            if let Err(e) = service.run().await {
                tracing::error!("IPLD Resolver Service failed: {e:#}")
            }
        });

        tracing::info!("starting the IPLD Resolver...");
        tokio::spawn(async move { resolver.run().await });
    } else {
        tracing::info!("IPLD Resolver disabled.")
    }

    let (parent_finality_provider, ipc_tuple) = if topdown_enabled {
        info!("topdown finality enabled");
        let topdown_config = settings.ipc.topdown_config()?;
        let mut config = fendermint_vm_topdown::Config::new(
            topdown_config.chain_head_delay,
            topdown_config.polling_interval,
            topdown_config.exponential_back_off,
            topdown_config.exponential_retry_limit,
        )
        .with_proposal_delay(topdown_config.proposal_delay)
        .with_max_proposal_range(topdown_config.max_proposal_range);

        if let Some(v) = topdown_config.max_cache_blocks {
            info!(value = v, "setting max cache blocks");
            config = config.with_max_cache_blocks(v);
        }

        let ipc_provider = Arc::new(make_ipc_provider_proxy(&settings)?);
        let finality_provider =
            CachedFinalityProvider::uninitialized(config.clone(), ipc_provider.clone()).await?;
        let p = Arc::new(Toggle::enabled(finality_provider));
        (p, Some((ipc_provider, config)))
    } else {
        info!("topdown finality disabled");
        (Arc::new(Toggle::disabled()), None)
    };

    // Start a snapshot manager in the background.
    let snapshots = if settings.snapshots.enabled {
        let (manager, client) = SnapshotManager::new(
            state_store.clone(),
            SnapshotParams {
                snapshots_dir: settings.snapshots_dir(),
                download_dir: settings.snapshots.download_dir(),
                block_interval: settings.snapshots.block_interval,
                chunk_size: settings.snapshots.chunk_size_bytes,
                hist_size: settings.snapshots.hist_size,
                last_access_hold: settings.snapshots.last_access_hold,
                sync_poll_interval: settings.snapshots.sync_poll_interval,
            },
        )
        .context("failed to create snapshot manager")?;

        tracing::info!("starting the SnapshotManager...");
        let tendermint_client = tendermint_client.clone();
        tokio::spawn(async move { manager.run(tendermint_client).await });

        Some(client)
    } else {
        info!("snapshots disabled");
        None
    };

    let app: App<_, _, AppStore, _> = App::new(
        AppConfig {
            app_namespace: ns.app,
            state_hist_namespace: ns.state_hist,
            state_hist_size: settings.db.state_hist_size,
            builtin_actors_bundle: settings.builtin_actors_bundle(),
            custom_actors_bundle: settings.custom_actors_bundle(),
            halt_height: settings.halt_height,
        },
        db,
        state_store,
        interpreter,
        ChainEnv {
            checkpoint_pool,
            parent_finality_provider: parent_finality_provider.clone(),
            parent_finality_votes: parent_finality_votes.clone(),
        },
        snapshots,
    )?;

    if let Some((agent_proxy, config)) = ipc_tuple {
        let app_parent_finality_query = AppParentFinalityQuery::new(app.clone());
        tokio::spawn(async move {
            match launch_polling_syncer(
                app_parent_finality_query,
                config,
                parent_finality_provider,
                parent_finality_votes,
                agent_proxy,
                tendermint_client,
            )
            .await
            {
                Ok(_) => {}
                Err(e) => tracing::error!("cannot launch polling syncer: {e}"),
            }
        });
    }

    // Start the metrics on a background thread.
    if let Some(registry) = metrics_registry {
        info!(
            listen_addr = settings.metrics.listen.to_string(),
            "serving metrics"
        );
        let mut builder = prometheus_exporter::Builder::new(settings.metrics.listen.try_into()?);
        builder.with_registry(registry);
        let _ = builder.start().context("failed to start metrics server")?;
    } else {
        info!("metrics disabled");
    }

    let service = ApplicationService(app);

    // Split it into components.
    let (consensus, mempool, snapshot, info) =
        tower_abci::v038::split::service(service, settings.abci.bound);

    // Hand those components to the ABCI server. This is where tower layers could be added.
    // TODO: Check out the examples about load shedding in `info` requests.
    let server = tower_abci::v038::Server::builder()
        .consensus(
            // Limiting the concurrency to 1 here because the `AplicationService::poll_ready` always
            // reports `Ready`, because it doesn't know which request it's going to get.
            // Not limiting the concurrency to 1 can lead to transactions being applied
            // in different order across nodes. The buffer size has to be large enough
            // to allow all in-flight requests to not block message handling in
            // `tower_abci::Connection::run`, which could lead to deadlocks.
            // With ABCI++ we need to be able to handle all block transactions plus the begin/end/commit
            // around it. With ABCI 2.0 we'll get the block as a whole, which makes this easier.
            ServiceBuilder::new()
                .buffer(settings.abci.block_max_msgs + 3)
                .concurrency_limit(1)
                .service(consensus),
        )
        .snapshot(snapshot)
        .mempool(mempool)
        .info(info)
        .finish()
        .context("error creating ABCI server")?;

    // Run the ABCI server.
    server
        .listen_tcp(settings.abci.listen.to_string())
        .await
        .map_err(|e| anyhow!("error listening: {e}"))?;

    Ok(())
}

/// Open database with all
fn open_db(settings: &Settings, ns: &Namespaces) -> anyhow::Result<RocksDb> {
    let path = settings.data_dir().join("rocksdb");
    info!(
        path = path.to_string_lossy().into_owned(),
        "opening database"
    );
    let config = RocksDbConfig {
        compaction_style: settings.db.compaction_style.to_string(),
        ..Default::default()
    };
    let db = RocksDb::open_cf(path, &config, ns.values().iter())?;
    Ok(db)
}

fn make_resolver_service(
    settings: &Settings,
    db: RocksDb,
    state_store: NamespaceBlockstore,
    bit_store_ns: String,
) -> anyhow::Result<ipc_ipld_resolver::Service<libipld::DefaultParams, AppVote>> {
    // Blockstore for Bitswap.
    let bit_store = NamespaceBlockstore::new(db, bit_store_ns).context("error creating bit DB")?;

    // Blockstore for Bitswap with a fallback on the actor store for reads.
    let bitswap_store = BitswapBlockstore::new(state_store, bit_store);

    let config = to_resolver_config(settings).context("error creating resolver config")?;

    let service = ipc_ipld_resolver::Service::new(config, bitswap_store)
        .context("error creating IPLD Resolver Service")?;

    Ok(service)
}

fn make_ipc_provider_proxy(settings: &Settings) -> anyhow::Result<IPCProviderProxy> {
    let topdown_config = settings.ipc.topdown_config()?;
    let subnet = ipc_provider::config::Subnet {
        id: settings
            .ipc
            .subnet_id
            .parent()
            .ok_or_else(|| anyhow!("subnet has no parent"))?,
        config: SubnetConfig::Fevm(EVMSubnet {
            provider_http: topdown_config
                .parent_http_endpoint
                .to_string()
                .parse()
                .unwrap(),
            provider_timeout: topdown_config.parent_http_timeout,
            auth_token: topdown_config.parent_http_auth_token.as_ref().cloned(),
            registry_addr: topdown_config.parent_registry,
            gateway_addr: topdown_config.parent_gateway,
        }),
    };
    info!("init ipc provider with subnet: {}", subnet.id);

    let ipc_provider = IpcProvider::new_with_subnet(None, subnet)?;
    IPCProviderProxy::new(ipc_provider, settings.ipc.subnet_id.clone())
}

fn to_resolver_config(settings: &Settings) -> anyhow::Result<ipc_ipld_resolver::Config> {
    use ipc_ipld_resolver::{
        Config, ConnectionConfig, ContentConfig, DiscoveryConfig, MembershipConfig, NetworkConfig,
    };

    let r = &settings.resolver;

    let local_key: Keypair = {
        let path = r.network.local_key(settings.home_dir());
        let sk = read_secret_key(&path)?;
        let sk = secp256k1::SecretKey::try_from_bytes(sk.serialize())?;
        secp256k1::Keypair::from(sk).into()
    };

    let network_name = format!(
        "ipld-resolver-{}-{}",
        settings.ipc.subnet_id.root_id(),
        r.network.network_name
    );

    let config = Config {
        connection: ConnectionConfig {
            listen_addr: r.connection.listen_addr.clone(),
            external_addresses: r.connection.external_addresses.clone(),
            expected_peer_count: r.connection.expected_peer_count,
            max_incoming: r.connection.max_incoming,
            max_peers_per_query: r.connection.max_peers_per_query,
            event_buffer_capacity: r.connection.event_buffer_capacity,
        },
        network: NetworkConfig {
            local_key,
            network_name,
        },
        discovery: DiscoveryConfig {
            static_addresses: r.discovery.static_addresses.clone(),
            target_connections: r.discovery.target_connections,
            enable_kademlia: r.discovery.enable_kademlia,
        },
        membership: MembershipConfig {
            static_subnets: r.membership.static_subnets.clone(),
            max_subnets: r.membership.max_subnets,
            publish_interval: r.membership.publish_interval,
            min_time_between_publish: r.membership.min_time_between_publish,
            max_provider_age: r.membership.max_provider_age,
        },
        content: ContentConfig {
            rate_limit_bytes: r.content.rate_limit_bytes,
            rate_limit_period: r.content.rate_limit_period,
        },
    };

    Ok(config)
}

fn to_address(sk: &SecretKey, kind: &AccountKind) -> anyhow::Result<Address> {
    let pk = sk.public_key().serialize();
    match kind {
        AccountKind::Regular => Ok(Address::new_secp256k1(&pk)?),
        AccountKind::Ethereum => Ok(Address::from(EthAddress::new_secp256k1(&pk)?)),
    }
}

async fn dispatch_resolver_events(
    mut rx: tokio::sync::broadcast::Receiver<ResolverEvent<AppVote>>,
    parent_finality_votes: VoteTally,
    topdown_enabled: bool,
) {
    loop {
        match rx.recv().await {
            Ok(event) => match event {
                ResolverEvent::ReceivedPreemptive(_, _) => {}
                ResolverEvent::ReceivedVote(vote) => {
                    dispatch_vote(*vote, &parent_finality_votes, topdown_enabled).await;
                }
            },
            Err(RecvError::Lagged(n)) => {
                tracing::warn!("the resolver service skipped {n} gossip events")
            }
            Err(RecvError::Closed) => {
                tracing::error!("the resolver service stopped receiving gossip");
                return;
            }
        }
    }
}

async fn dispatch_vote(
    vote: VoteRecord<AppVote>,
    parent_finality_votes: &VoteTally,
    topdown_enabled: bool,
) {
    match vote.content {
        AppVote::ParentFinality(f) => {
            if !topdown_enabled {
                tracing::debug!("ignoring vote; topdown disabled");
                return;
            }
            let res = atomically_or_err(|| {
                parent_finality_votes.add_vote(
                    vote.public_key.clone(),
                    f.height,
                    f.block_hash.clone(),
                )
            })
            .await;

            let added = match res {
                Ok(added) => {
                    added
                }
                Err(e @ VoteError::Equivocation(_, _, _, _)) => {
                    tracing::warn!(error = e.to_string(), "failed to handle vote");
                    false
                }
                Err(e @ (
                      VoteError::Uninitialized // early vote, we're not ready yet
                    | VoteError::UnpoweredValidator(_) // maybe arrived too early or too late, or spam
                    | VoteError::UnexpectedBlock(_, _) // won't happen here
                )) => {
                    tracing::debug!(error = e.to_string(), "failed to handle vote");
                    false
                }
            };

            let block_height = f.height;
            let block_hash = &hex::encode(&f.block_hash);
            let validator = &format!("{:?}", vote.public_key);

            if added {
                emit!(
                    DEBUG,
                    ParentFinalityVoteAdded {
                        block_height,
                        block_hash,
                        validator,
                    }
                )
            } else {
                emit!(
                    DEBUG,
                    ParentFinalityVoteIgnored {
                        block_height,
                        block_hash,
                        validator,
                    }
                )
            }
        }
    }
}
