// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Example ABCI application, an in-memory key-value store.

use async_stm::{atomically, TVar};
use async_trait::async_trait;
use fendermint_abci::{
    util::take_until_max_size, AbciResult as Result, Application, ApplicationService,
};
use structopt::StructOpt;
use tendermint::abci::{request, response, Event, EventAttributeIndexExt};
use tower::ServiceBuilder;
use tower_abci::{split, v037::Server};
use tracing::{info, Level};

// For the sake of example, sho the relationship between buffering, concurrency and block size.
const MAX_TXNS: usize = 100;

/// In-memory, hashmap-backed key-value store ABCI application.
///
/// Using STM just to see if it works. It's obviously an overkill here.
#[derive(Clone)]
struct KVStore {
    store: TVar<im::HashMap<String, String>>,
    height: TVar<u32>,
    app_hash: TVar<[u8; 8]>,
}

impl KVStore {
    pub fn new() -> Self {
        Self {
            store: TVar::new(im::HashMap::new()),
            height: TVar::new(Default::default()),
            app_hash: TVar::new(Default::default()),
        }
    }
}

#[async_trait]
impl Application for KVStore {
    async fn info(&self, _request: request::Info) -> Result<response::Info> {
        let (height, app_hash) = atomically(|| {
            let height = self.height.read_clone()?.into();
            let app_hash = self.app_hash.read()?.to_vec().try_into().unwrap();
            Ok((height, app_hash))
        })
        .await;

        Ok(response::Info {
            data: "kvstore-example".to_string(),
            version: "0.1.0".to_string(),
            app_version: 1,
            last_block_height: height,
            last_block_app_hash: app_hash,
        })
    }

    async fn query(&self, request: request::Query) -> Result<response::Query> {
        let key = String::from_utf8(request.data.to_vec()).unwrap();
        let (value, log) = atomically(|| match self.store.read()?.get(&key) {
            Some(v) => Ok((v.clone(), "exists".to_string())),
            None => Ok(("".to_string(), "does not exist".to_string())),
        })
        .await;

        Ok(response::Query {
            log,
            key: key.into_bytes().into(),
            value: value.into_bytes().into(),
            ..Default::default()
        })
    }

    async fn prepare_proposal(
        &self,
        request: request::PrepareProposal,
    ) -> Result<response::PrepareProposal> {
        let mut txs = take_until_max_size(request.txs, request.max_tx_bytes.try_into().unwrap());

        // Enfore transaciton limit so that we don't have a problem with buffering.
        txs.truncate(MAX_TXNS);

        Ok(response::PrepareProposal { txs })
    }

    async fn process_proposal(
        &self,
        request: request::ProcessProposal,
    ) -> Result<response::ProcessProposal> {
        if request.txs.len() > MAX_TXNS {
            Ok(response::ProcessProposal::Reject)
        } else {
            Ok(response::ProcessProposal::Accept)
        }
    }

    async fn deliver_tx(&self, request: request::DeliverTx) -> Result<response::DeliverTx> {
        let tx = String::from_utf8(request.tx.to_vec()).unwrap();
        let (key, value) = match tx.split('=').collect::<Vec<_>>() {
            k if k.len() == 1 => (k[0], k[0]),
            kv => (kv[0], kv[1]),
        };

        atomically(|| {
            self.store.update(|mut store| {
                store.insert(key.into(), value.into());
                store
            })
        })
        .await;

        info!(?key, ?value, "update");

        Ok(response::DeliverTx {
            events: vec![Event::new(
                "app",
                vec![
                    ("key", key).index(),
                    ("index_key", "index is working").index(),
                    ("noindex_key", "index is working").no_index(),
                ],
            )],
            ..Default::default()
        })
    }

    async fn commit(&self) -> Result<response::Commit> {
        let (retain_height, app_hash) = atomically(|| {
            // As in the other kvstore examples, just use store.len() as the "hash"
            let app_hash = (self.store.read()?.len() as u64).to_be_bytes();
            self.app_hash.replace(app_hash)?;
            let retain_height = self.height.modify(|h| (h + 1, h))?;
            Ok((retain_height.into(), app_hash.to_vec().try_into().unwrap()))
        })
        .await;

        info!(?retain_height, "commit");

        Ok(response::Commit {
            data: app_hash,
            retain_height,
        })
    }
}

#[derive(Debug, StructOpt)]
struct Opt {
    /// Bind the TCP server to this host.
    #[structopt(short, long, default_value = "127.0.0.1")]
    host: String,

    /// Bind the TCP server to this port.
    #[structopt(short, long, default_value = "26658")]
    port: u16,

    /// Increase output logging verbosity to DEBUG level.
    #[structopt(short, long)]
    verbose: bool,
}

impl Opt {
    pub fn log_level(&self) -> Level {
        if self.verbose {
            Level::DEBUG
        } else {
            Level::INFO
        }
    }
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();

    tracing_subscriber::fmt()
        .with_max_level(opt.log_level())
        .init();

    // Construct our ABCI application.
    let service = ApplicationService(KVStore::new());

    // Split it into components.
    let (consensus, mempool, snapshot, info) = split::service(service, 1);

    // Hand those components to the ABCI server. This is where tower layers could be added.
    let server = Server::builder()
        .consensus(
            // Because message handling is asynchronous, we must limit the concurrency of `consensus` to 1,
            // otherwise transactions can be executed in an arbitrary order. `buffer` is required to avoid
            // deadlocks in the connection handler; in ABCI++ (pre 2.0) we need to allow for all potential
            // messages in the block, plus the surrounding begin/end/commit methods to be pipelined. The
            // message limit is enforced in proposal preparation and processing.
            ServiceBuilder::new()
                .buffer(MAX_TXNS + 3)
                .concurrency_limit(1)
                .service(consensus),
        )
        .snapshot(snapshot)
        .mempool(mempool)
        .info(info)
        .finish()
        .unwrap();

    // Run the ABCI server.
    server
        .listen(format!("{}:{}", opt.host, opt.port))
        .await
        .unwrap();
}
