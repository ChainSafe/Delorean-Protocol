// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! Helper commands for interacting with the Delorean/CETF actor via RPC
//!
//! The example assumes that Tendermint and Fendermint have been started
//! and are running locally.
//!
//! # Usage
//! ```text
//! cargo run --example delorean -- --secret-key test-data/keys/volvo.sk queue-tag
//! ```

use std::any;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{anyhow, Context};
use bls_signatures::Serialize;
use bytes::Bytes;
use cetf_actor::State as CetfActorState;
use clap::{Parser, Subcommand};
use ethers::abi::Tokenizable;
use ethers::prelude::*;
use fendermint_actor_cetf::state::DEFAULT_HAMT_CONFIG;
use fendermint_actor_cetf::{self as cetf_actor, BlsSignature};
use delorean_cli::RemoteBlockstore;
use fendermint_rpc::query::{QueryClient, QueryResponse};
use fendermint_vm_actor_interface::eam;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::{CborStore, RawBytes};
use fvm_shared::address::Address;
use fvm_shared::chainid::ChainID;
use fvm_shared::econ::TokenAmount;
use k256::sha2::{Digest, Sha256};
use lazy_static::lazy_static;
use tendermint_rpc::Url;
use tracing::Level;

use fendermint_rpc::client::FendermintClient;
use fendermint_rpc::message::{GasParams, SignedMessageFactory};
use fendermint_rpc::tx::{CallClient, TxClient, TxCommit};

type MockProvider = ethers::providers::Provider<ethers::providers::MockProvider>;
type MockContractCall<T> = ethers::prelude::ContractCall<MockProvider, T>;

const DEMO_CONTRACT_SPEC_JSON: &str =
    include_str!("../../../../contracts/out/Demo.sol/DeloreanDemo.json");

lazy_static! {
    /// Default gas params based on the testkit.
    static ref GAS_PARAMS: GasParams = GasParams {
        gas_limit: 10_000_000_000,
        gas_fee_cap: TokenAmount::default(),
        gas_premium: TokenAmount::default(),
    };
}

abigen!(
    DeloreanContract,
    r#"[
        {
            "type": "function",
            "name": "releaseKey",
            "inputs": [],
            "outputs": [
                {
                    "name": "",
                    "type": "bool",
                    "internalType": "bool"
                }
            ],
            "stateMutability": "nonpayable"
        },
        {
            "type": "function",
            "name": "signingTag",
            "inputs": [],
            "outputs": [
                {
                    "name": "",
                    "type": "bytes32",
                    "internalType": "bytes32"
                }
            ],
            "stateMutability": "nonpayable"
        }
    ]"#
);

#[derive(Parser, Debug)]
pub struct Options {
    /// The URL of the Tendermint node's RPC endpoint.
    #[arg(
        long,
        short,
        default_value = "http://127.0.0.1:26657",
        env = "TENDERMINT_RPC_URL"
    )]
    pub url: Url,

    /// Enable DEBUG logs.
    #[arg(long, short)]
    pub verbose: bool,

    #[command(subcommand)]
    command: Commands,

    /// Path to the secret key to deploy with, expected to be in Base64 format,
    /// and that it has a corresponding f410 account in genesis.
    #[arg(long, short, env = "DELORIAN_SECRET_KEY")]
    pub secret_key: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Commands {
    RegisterBls {
        #[arg(long, short)]
        bls_secret_key: PathBuf,
    },
    QueueTag,
    DeployDemoContract,
    CallReleaseKeys {
        address: String,
    },
    RegisteredKeys,
    Encrypt {
        contract_address: String,
        #[arg(long, short)]
        output: PathBuf,
    },
    Decrypt {
        contract_address: String,
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    TestIfHeightsAreSignedProperly,
}

impl Options {
    pub fn log_level(&self) -> Level {
        if self.verbose {
            Level::DEBUG
        } else {
            Level::INFO
        }
    }
}

/// See the module docs for how to run.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts: Options = Options::parse();

    tracing_subscriber::fmt()
        .with_max_level(opts.log_level())
        .init();

    let client = FendermintClient::new_http(opts.url, None).expect("error creating client");
    let store = RemoteBlockstore::new(client.clone());

    let sk =
        SignedMessageFactory::read_secret_key(&opts.secret_key).expect("error reading secret key");

    let pk = sk.public_key();

    let f1_addr = Address::new_secp256k1(&pk.serialize()).expect("valid public key");

    // Query the account nonce from the state, so it doesn't need to be passed as an arg.
    let sn = sequence(&client, &f1_addr)
        .await
        .expect("error getting sequence");

    // Query the chain ID, so it doesn't need to be passed as an arg.
    let chain_id = client
        .state_params(FvmQueryHeight::default())
        .await
        .expect("error getting state params")
        .value
        .chain_id;

    let mf = SignedMessageFactory::new(sk, f1_addr, sn, ChainID::from(chain_id));

    let mut client = client.bind(mf);

    match opts.command {
        Commands::RegisterBls { bls_secret_key } => {
            let bls_sk = {
                let b64 = std::fs::read_to_string(&bls_secret_key)
                    .expect("failed to read bls secret key");
                bls_signatures::PrivateKey::from_bytes(
                    &fendermint_crypto::from_b64(&b64)
                        .expect("failed to decode b64 bls secret key"),
                )
                .expect("failed to parse bls secret key")
            };

            let bls_pk = bls_sk.public_key();

            let res = TxClient::<TxCommit>::transaction(
                &mut client,
                fendermint_vm_actor_interface::cetf::CETFSYSCALL_ACTOR_ADDR,
                cetf_actor::Method::AddValidator as u64,
                RawBytes::serialize(cetf_actor::AddValidatorParams {
                    address: f1_addr,
                    public_key: fendermint_actor_cetf::BlsPublicKey(
                        bls_pk
                            .as_bytes()
                            .try_into()
                            .expect("Failed to convert BLS public key to bytes"),
                    ),
                })
                .expect("failed to serialize params"),
                TokenAmount::from_whole(0),
                GAS_PARAMS.clone(),
            )
            .await
            .expect("transfer failed");

            assert!(res.response.check_tx.code.is_ok(), "check is ok");
            assert!(res.response.tx_result.code.is_ok(), "deliver is ok");
            assert!(res.return_data.is_some());
        }
        Commands::RegisteredKeys => {
            let QueryResponse { height, value } = client
                .actor_state(
                    &fendermint_vm_actor_interface::cetf::CETFSYSCALL_ACTOR_ADDR,
                    FvmQueryHeight::default(),
                )
                .await
                .expect("failed to get cetf actor state");

            let (id, act_state) = value.expect("cetf actor state not found");
            tracing::info!("Get Cetf State (id: {}) at height {}", id, height);
            let state: CetfActorState = store
                .get_cbor(&act_state.state)
                .expect("failed to get cetf actor")
                .expect("no actor state found");

            let validator_map = cetf_actor::state::ValidatorBlsPublicKeyMap::load(
                store,
                &state.validators,
                DEFAULT_HAMT_CONFIG,
                "load validator hamt",
            )
            .expect("failed to load validator hamt");
            validator_map
                .for_each(|k, v| {
                    tracing::info!("Validator: {}, Bls: {:?}", k, v);
                    Ok(())
                })
                .expect("failed to iterate validator hamt");
        }
        Commands::QueueTag => {
            let to_queue: [u8; 32] = std::array::from_fn(|i| i as u8);
            let params = RawBytes::serialize(cetf_actor::EnqueueTagParams {
                tag: to_queue.into(),
            })
            .expect("failed to serialize params");
            tracing::info!("CBOR encoded input should look like: {:?}", params);

            let res = TxClient::<TxCommit>::transaction(
                &mut client,
                fendermint_vm_actor_interface::cetf::CETFSYSCALL_ACTOR_ADDR,
                cetf_actor::Method::EnqueueTag as u64,
                params,
                TokenAmount::from_whole(0),
                GAS_PARAMS.clone(),
            )
            .await
            .expect("transfer failed");

            assert!(res.response.check_tx.code.is_ok(), "check is ok");
            assert!(res.response.tx_result.code.is_ok(), "deliver is ok");
            assert!(res.return_data.is_some());
            let scheduled_epoch: u64 = res
                .return_data
                .expect("no return data")
                .deserialize()
                .expect("failed to deserialize return data");
            tracing::info!("Scheduled epoch: {}", scheduled_epoch);
        }
        Commands::DeployDemoContract => {
            let spec: serde_json::Value = serde_json::from_str(DEMO_CONTRACT_SPEC_JSON)?;
            let example_contract = hex::decode(
                &spec["bytecode"]["object"]
                    .as_str()
                    .expect("missing bytecode")
                    .trim_start_matches("0x"),
            )?;

            tracing::info!("Deploying Example Contract");

            let res = TxClient::<TxCommit>::fevm_create(
                &mut client,
                Bytes::from(example_contract),
                Bytes::default(),
                TokenAmount::default(),
                GAS_PARAMS.clone(),
            )
            .await
            .expect("error deploying contract");

            tracing::info!(tx_hash = ?res.response.hash, "deployment transaction");

            let ret = res
                .return_data
                .ok_or(anyhow!(
                    "no CreateReturn data; response was {:?}",
                    res.response
                ))
                .expect("failed to get CreateReturn data");
            let address = ret.eth_address;
            tracing::info!(address = ?address, "contract deployed");
        }
        Commands::CallReleaseKeys { address } => {
            let contract = delorean_contract(&address);
            let call = contract.release_key();

            tracing::info!("Calling releaeKeys on contract at address {}", address);
            let result: bool = invoke_or_call_contract(&mut client, &address, call, true).await?;

            tracing::info!(result = ?result, "contract call result");
        }
        Commands::Encrypt { contract_address, output } => {
            let signing_tag = retrieve_signing_tag(&mut client, &contract_address).await?;
            tracing::info!("Retrieved signing tag 0x{} from contract", hex::encode(&signing_tag));

            let agg_pubkey = get_agg_pubkey(&client, &store).await?;
            tracing::info!("Computed aggregate BLS pubkey 0x{}", hex::encode(&agg_pubkey.as_bytes()));

            tracing::info!("Encrypting...");

            // encrypt whatever is on std-in into our armor writer
            let mut armored = tlock_age::armor::ArmoredWriter::wrap_output(vec![]).unwrap();
            tlock_age::encrypt(
                &mut armored,
                std::io::stdin().lock(),
                &[0x0; 32], // I think this can be anything..
                &agg_pubkey.as_bytes(),
                signing_tag,
            )?;
            let encrypted = armored.finish().unwrap();

            // write the encrypted data to the output file
            std::fs::write(&output, &encrypted).expect("failed to write output file");
            tracing::info!("Done!");

        }
        Commands::Decrypt { contract_address, output } => {
            let signing_tag = retrieve_signing_tag(&mut client, &contract_address).await?;
            tracing::info!("Retrieved signing tag 0x{} from contract", hex::encode(&signing_tag));
            tracing::info!("Attempting to retrieve signature for tag");

            let sig_bytes = get_signature_for_tag(&client, &store, signing_tag).await?;
            tracing::info!("Got key/signature 0x{}", hex::encode(sig_bytes.0));

            let mut decrypted = vec![];
            tlock_age::decrypt(
                &mut decrypted,
                std::io::stdin().lock(),
                &[0x0; 32],
                &sig_bytes.0,
            )?;
            if let Some(output) = output {
                std::fs::write(&output, &decrypted).expect("failed to write output file");
            } else {
                std::io::stdout().write_all(&decrypted).expect("failed to write to stdout");
            }
        }
        Commands::TestIfHeightsAreSignedProperly => {
            let QueryResponse { height, value } = client
                .actor_state(
                    &fendermint_vm_actor_interface::cetf::CETFSYSCALL_ACTOR_ADDR,
                    FvmQueryHeight::default(),
                )
                .await
                .expect("failed to get cetf actor state");

            let (id, act_state) = value.expect("cetf actor state not found");
            tracing::info!("Get Cetf State (id: {}) at height {}", id, height);
            let state: CetfActorState = store
                .get_cbor(&act_state.state)
                .expect("failed to get cetf actor")
                .expect("no actor state found");

            let height: u64 = height.into();
            let height = height - 1u64;
            // Get all the validators BLS keys
            let mut bls_keys_bytes = vec![];
            let validator_map = cetf_actor::state::ValidatorBlsPublicKeyMap::load(
                store.clone(),
                &state.validators,
                DEFAULT_HAMT_CONFIG,
                "load validator hamt",
            )
            .expect("failed to load validator hamt");
            validator_map
                .for_each(|_k, v| {
                    bls_keys_bytes.push(*v);
                    Ok(())
                })
                .expect("failed to iterate validator hamt");

            // Find the Signature of the Hashed Tag
            let tag_bytes = height.to_be_bytes().to_vec();
            let mut hasher = Sha256::new();
            hasher.update(&tag_bytes);

            let digest: [u8; 32] = hasher.finalize().into();

            let signed_hashed_tag = cetf_actor::state::SignedHashedTagMap::load(
                store.clone(),
                &state.signed_hashed_tags,
                DEFAULT_HAMT_CONFIG,
                "load signed hashed tags",
            )
            .expect("failed to load signed hashed tags");

            let sig_bytes: BlsSignature = *signed_hashed_tag
                .get(&digest.into())
                .expect("failed to get signature from signed hashed tag")
                .expect("signature not found");

            let sig = bls_signatures::Signature::from_bytes(&sig_bytes.0)
                .expect("failed to parse signature from bytes");
            let pub_keys = bls_keys_bytes
                .iter()
                .map(|b| {
                    bls_signatures::PublicKey::from_bytes(&b.0)
                        .expect("failed to parse public key from bytes")
                })
                .collect::<Vec<_>>();
            // tracing::info!("Public Keys: {:?}", pub_keys);
            tracing::info!("Tag: {:?}", tag_bytes);
            tracing::info!("Hashed Tag: {:?}", digest);
            tracing::info!("Signature: {:?}", sig_bytes.0);

            assert!(
                bls_signatures::verify_messages(&sig, &[tag_bytes.as_slice()], &pub_keys),
                "Signature is invalid"
            );
        }
    }
    Ok(())
}

/// Invoke FEVM through Tendermint with the calldata encoded by ethers, decoding the result into the expected type.
async fn invoke_or_call_contract<T: Tokenizable>(
    client: &mut (impl TxClient<TxCommit> + CallClient),
    contract_eth_addr: &str,
    call: MockContractCall<T>,
    in_transaction: bool,
) -> anyhow::Result<T> {
    let calldata: ethers::types::Bytes = call
        .calldata()
        .expect("calldata should contain function and parameters");

    let contract_addr = eth_addr_to_eam(contract_eth_addr);

    // We can perform the read as a distributed transaction (if we don't trust any particular node to give the right answer),
    // or we can send a query with the same message and get a result without involving a transaction.
    let return_data = if in_transaction {
        let res = client
            .fevm_invoke(
                contract_addr,
                calldata.0,
                TokenAmount::default(),
                GAS_PARAMS.clone(),
            )
            .await
            .context("failed to invoke FEVM")?;

        // tracing::info!(tx_hash = ?res.response.hash, "invoked transaction");

        res.return_data
    } else {
        let res = client
            .fevm_call(
                contract_addr,
                calldata.0,
                TokenAmount::default(),
                GAS_PARAMS.clone(),
                FvmQueryHeight::default(),
            )
            .await
            .context("failed to call FEVM")?;

        res.return_data
    };

    let bytes = return_data.ok_or(anyhow!("Contract returned error. Key release denied."))?;

    let res = decode_function_data(&call.function, bytes, false)
        .context("error deserializing return data")?;

    Ok(res)
}

/// Get the next sequence number (nonce) of an account.
async fn sequence(client: &impl QueryClient, addr: &Address) -> anyhow::Result<u64> {
    let state = client
        .actor_state(&addr, FvmQueryHeight::default())
        .await
        .context("failed to get actor state")?;

    match state.value {
        Some((_id, state)) => Ok(state.sequence),
        None => Err(anyhow!("cannot find actor {addr}")),
    }
}

/// Create an instance of the statically typed contract client.
fn delorean_contract(contract_eth_addr: &str) -> DeloreanContract<MockProvider> {
    // A dummy client that we don't intend to use to call the contract or send transactions.
    let (client, _mock) = ethers::providers::Provider::mocked();
    let contract_h160_addr = ethers::core::types::Address::from_slice(
        hex::decode(contract_eth_addr.trim_start_matches("0x"))
            .unwrap()
            .as_slice(),
    );
    let contract = DeloreanContract::new(contract_h160_addr, std::sync::Arc::new(client));
    contract
}

/// Retrive the signing tag from a deployed Demo contract given its address
async fn retrieve_signing_tag(
    client: &mut (impl TxClient<TxCommit> + CallClient),
    contract_eth_addr: &str,
) -> anyhow::Result<[u8; 32]> {
    let contract = delorean_contract(contract_eth_addr);
    let call = contract.signing_tag();
    let signing_tag: [u8; 32] = invoke_or_call_contract(client, contract_eth_addr, call, true)
        .await
        .context("failed to call contract")?;
    Ok(signing_tag)
}

async fn get_agg_pubkey(
    client: &impl QueryClient,
    store: &RemoteBlockstore<FendermintClient>,
) -> anyhow::Result<bls_signatures::PublicKey> {
    let QueryResponse { height, value } = client
        .actor_state(
            &fendermint_vm_actor_interface::cetf::CETFSYSCALL_ACTOR_ADDR,
            FvmQueryHeight::default(),
        )
        .await
        .expect("failed to get cetf actor state");

    let (id, act_state) = value.expect("cetf actor state not found");
    // tracing::info!("Get Cetf State (id: {}) at height {}", id, height);
    let state: CetfActorState = store
        .get_cbor(&act_state.state)
        .expect("failed to get cetf actor")
        .expect("no actor state found");

    // Get all the validators BLS keys
    let mut bls_keys_bytes = vec![];
    let validator_map = cetf_actor::state::ValidatorBlsPublicKeyMap::load(
        store.clone(),
        &state.validators,
        DEFAULT_HAMT_CONFIG,
        "load validator hamt",
    )
    .expect("failed to load validator hamt");
    validator_map
        .for_each(|_k, v| {
            bls_keys_bytes.push(*v);
            Ok(())
        })
        .expect("failed to iterate validator hamt");

    let pub_keys = bls_keys_bytes
        .iter()
        .map(|b| {
            bls_signatures::PublicKey::from_bytes(&b.0)
                .expect("failed to parse public key from bytes")
        })
        .collect::<Vec<_>>();

    Ok(bls_signatures::aggregate_keys(&pub_keys).expect("failed to aggregate public keys"))
}

async fn get_signature_for_tag(
    client: &impl QueryClient,
    store: &RemoteBlockstore<FendermintClient>,
    signing_tag: [u8; 32],
) -> anyhow::Result<cetf_actor::BlsSignature> {
    let QueryResponse { height, value } = client
        .actor_state(
            &fendermint_vm_actor_interface::cetf::CETFSYSCALL_ACTOR_ADDR,
            FvmQueryHeight::default(),
        )
        .await
        .expect("failed to get cetf actor state");

    let (id, act_state) = value.expect("cetf actor state not found");
    tracing::info!("Get Cetf State (id: {}) at height {}", id, height);
    let state: CetfActorState = store
        .get_cbor(&act_state.state)
        .expect("failed to get cetf actor")
        .expect("no actor state found");

    let signed_hashed_tag = cetf_actor::state::SignedHashedTagMap::load(
        store.clone(),
        &state.signed_hashed_tags,
        DEFAULT_HAMT_CONFIG,
        "load signed hashed tags",
    )
    .expect("failed to load signed hashed tags");

    Ok(*signed_hashed_tag
        .get(&signing_tag.into())
        .expect("failed to get signature from signed hashed tag")
        .expect("signature not found"))
}

fn eth_addr_to_eam(eth_addr: &str) -> Address {
    let eth_addr = hex::decode(eth_addr.trim_start_matches("0x")).expect("valid hex");
    Address::new_delegated(eam::EAM_ACTOR_ID, &eth_addr)
        .expect("ETH address to delegated should work")
}
