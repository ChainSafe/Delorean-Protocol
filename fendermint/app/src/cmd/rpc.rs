// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use anyhow::Context;
use async_trait::async_trait;
use bytes::Bytes;
use fendermint_app_options::genesis::AccountKind;
use fendermint_crypto::{to_b64, SecretKey};
use fendermint_rpc::client::BoundFendermintClient;
use fendermint_rpc::tx::{
    AsyncResponse, BoundClient, CallClient, CommitResponse, SyncResponse, TxAsync, TxClient,
    TxCommit, TxSync,
};
use fendermint_vm_core::chainid;
use fendermint_vm_message::chain::ChainMessage;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use fvm_shared::MethodNum;
use serde::Serialize;
use serde_json::json;
use tendermint::abci::response::DeliverTx;
use tendermint::block::Height;
use tendermint_rpc::HttpClient;

use fendermint_rpc::message::{GasParams, SignedMessageFactory};
use fendermint_rpc::{client::FendermintClient, query::QueryClient};
use fendermint_vm_actor_interface::eam::{self, CreateReturn, EthAddress};

use crate::cmd;
use crate::options::rpc::{BroadcastMode, FevmArgs, RpcFevmCommands, TransArgs};
use crate::options::rpc::{RpcArgs, RpcCommands, RpcQueryCommands};

use super::key::read_secret_key;

cmd! {
  RpcArgs(self) {
    let client = FendermintClient::new_http(self.url.clone(), self.proxy_url.clone())?;
    match self.command.clone() {
      RpcCommands::Query { height, command } => {
        let height = Height::try_from(height)?;
        query(client, height, command).await
      },
      RpcCommands::Transfer { args, to } => {
        transfer(client, args, to).await
      },
      RpcCommands::Transaction { args, to, method_number, params } => {
        transaction(client, args, to, method_number, params.clone()).await
      },
      RpcCommands::Fevm { args, command } => match command {
        RpcFevmCommands::Create { contract, constructor_args } => {
            fevm_create(client, args, contract, constructor_args).await
        }
        RpcFevmCommands::Invoke { args: FevmArgs { contract, method, method_args }} => {
            fevm_invoke(client, args, contract, method, method_args).await
        }
        RpcFevmCommands::Call { args: FevmArgs { contract, method, method_args }, height} => {
            let height = Height::try_from(height)?;
            fevm_call(client, args, contract, method, method_args, height).await
        }
        RpcFevmCommands::EstimateGas { args: FevmArgs { contract, method, method_args }, height} => {
            let height = Height::try_from(height)?;
            fevm_estimate_gas(client, args, contract, method, method_args, height).await
        }
      }
    }
  }
}

/// Run an ABCI query and print the results on STDOUT.
async fn query(
    client: FendermintClient,
    height: Height,
    command: RpcQueryCommands,
) -> anyhow::Result<()> {
    let height = FvmQueryHeight::from(height.value());
    match command {
        RpcQueryCommands::Ipld { cid } => match client.ipld(&cid, height).await? {
            Some(data) => println!("{}", to_b64(&data)),
            None => eprintln!("CID not found"),
        },
        RpcQueryCommands::ActorState { address } => {
            match client.actor_state(&address, height).await?.value {
                Some((id, state)) => {
                    let out = json! ({
                      "id": id,
                      "state": state,
                    });
                    print_json(&out)?;
                }
                None => {
                    eprintln!("actor not found")
                }
            }
        }
        RpcQueryCommands::StateParams => {
            let res = client.state_params(height).await?;
            let json = json!({ "response": res });
            print_json(&json)?;
        }
    };
    Ok(())
}

/// Create a client, make a call to Tendermint with a closure, then maybe extract some JSON
/// depending on the return value, finally print the result in JSON.
async fn broadcast_and_print<F, T, G>(
    client: FendermintClient,
    args: TransArgs,
    f: F,
    g: G,
) -> anyhow::Result<()>
where
    F: FnOnce(
        TransClient,
        TokenAmount,
        GasParams,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<BroadcastResponse<T>>> + Send>>,
    G: FnOnce(T) -> serde_json::Value,
    T: Sync + Send,
{
    let client = TransClient::new(client, &args)?;
    let gas_params = gas_params(&args);
    let res = f(client, args.value, gas_params).await?;
    let json = match res {
        BroadcastResponse::Async(res) => json!({"response": res.response}),
        BroadcastResponse::Sync(res) => json!({"response": res.response}),
        BroadcastResponse::Commit(res) => {
            let return_data = res.return_data.map(g).unwrap_or(serde_json::Value::Null);
            json!({"response": res.response, "return_data": return_data})
        }
    };
    print_json(&json)
}

/// Execute token transfer through RPC and print the response to STDOUT as JSON.
async fn transfer(client: FendermintClient, args: TransArgs, to: Address) -> anyhow::Result<()> {
    broadcast_and_print(
        client,
        args,
        |mut client, value, gas_params| {
            Box::pin(async move { client.transfer(to, value, gas_params).await })
        },
        |_| serde_json::Value::Null,
    )
    .await
}

/// Execute a transaction through RPC and print the response to STDOUT as JSON.
///
/// If there was any data returned it's rendered in hexadecimal format.
async fn transaction(
    client: FendermintClient,
    args: TransArgs,
    to: Address,
    method_num: MethodNum,
    params: RawBytes,
) -> anyhow::Result<()> {
    broadcast_and_print(
        client,
        args,
        |mut client, value, gas_params| {
            Box::pin(async move {
                client
                    .transaction(to, method_num, params, value, gas_params)
                    .await
            })
        },
        |data| serde_json::Value::String(hex::encode(data.bytes())),
    )
    .await
}

/// Deploy an EVM contract through RPC and print the response to STDOUT as JSON.
///
/// The returned EVM contract addresses are included as a JSON object.
async fn fevm_create(
    client: FendermintClient,
    args: TransArgs,
    contract: PathBuf,
    constructor_args: Bytes,
) -> anyhow::Result<()> {
    let contract_hex = std::fs::read_to_string(contract).context("failed to read contract")?;
    let contract_bytes = hex::decode(contract_hex).context("failed to parse contract from hex")?;
    let contract_bytes = Bytes::from(contract_bytes);

    broadcast_and_print(
        client,
        args,
        |mut client, value, gas_params| {
            Box::pin(async move {
                client
                    .fevm_create(contract_bytes, constructor_args, value, gas_params)
                    .await
            })
        },
        create_return_to_json,
    )
    .await
}

/// Invoke an EVM contract through RPC and print the response to STDOUT as JSON.
async fn fevm_invoke(
    client: FendermintClient,
    args: TransArgs,
    contract: Address,
    method: Bytes,
    method_args: Bytes,
) -> anyhow::Result<()> {
    let calldata = Bytes::from([method, method_args].concat());
    broadcast_and_print(
        client,
        args,
        |mut client, value, gas_params| {
            Box::pin(async move {
                client
                    .fevm_invoke(contract, calldata, value, gas_params)
                    .await
            })
        },
        |data| serde_json::Value::String(hex::encode(data)),
    )
    .await
}

/// Call an EVM contract through RPC and print the response to STDOUT as JSON.
async fn fevm_call(
    client: FendermintClient,
    args: TransArgs,
    contract: Address,
    method: Bytes,
    method_args: Bytes,
    height: Height,
) -> anyhow::Result<()> {
    let calldata = Bytes::from([method, method_args].concat());
    let mut client = TransClient::new(client, &args)?;
    let gas_params = gas_params(&args);
    let value = args.value;
    let height = FvmQueryHeight::from(height.value());

    let res = client
        .inner
        .fevm_call(contract, calldata, value, gas_params, height)
        .await?;

    let return_data = res
        .return_data
        .map(|bz| serde_json::Value::String(hex::encode(bz)))
        .unwrap_or(serde_json::Value::Null);

    let json = json!({"response": res.response, "return_data": return_data});

    print_json(&json)
}

/// Estimate the gas of an EVM call through RPC and print the response to STDOUT as JSON.
async fn fevm_estimate_gas(
    client: FendermintClient,
    args: TransArgs,
    contract: Address,
    method: Bytes,
    method_args: Bytes,
    height: Height,
) -> anyhow::Result<()> {
    let calldata = Bytes::from([method, method_args].concat());
    let mut client = TransClient::new(client, &args)?;
    let gas_params = gas_params(&args);
    let value = args.value;
    let height = FvmQueryHeight::from(height.value());

    let res = client
        .inner
        .fevm_estimate_gas(contract, calldata, value, gas_params, height)
        .await?;

    let json = json!({ "response": res });

    print_json(&json)
}

/// Print out pretty-printed JSON.
///
/// People can use `jq` to turn it into compact form if they want to save the results to a `.jsonline`
/// file, but the default of having human readable output seems more useful.
fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(&value)?;
    println!("{}", json);
    Ok(())
}

/// Print all the various addresses we can use to refer to an EVM contract.
fn create_return_to_json(ret: CreateReturn) -> serde_json::Value {
    // The only reference I can point to about how to use them are the integration tests:
    // https://github.com/filecoin-project/ref-fvm/pull/1507
    // IIRC to call the contract we need to use the `actor_address` or the `delegated_address` in `to`.
    json!({
        "actor_id": ret.actor_id,
        "actor_address": Address::new_id(ret.actor_id).to_string(),
        "actor_id_as_eth_address": hex::encode(eam::EthAddress::from_id(ret.actor_id).0),
        "eth_address": hex::encode(ret.eth_address.0),
        "delegated_address": ret.delegated_address().to_string(),
        "robust_address": ret.robust_address.map(|a| a.to_string())
    })
}

pub enum BroadcastResponse<T> {
    Async(AsyncResponse<T>),
    Sync(SyncResponse<T>),
    Commit(CommitResponse<T>),
}

struct BroadcastModeWrapper(BroadcastMode);

impl fendermint_rpc::tx::BroadcastMode for BroadcastModeWrapper {
    type Response<T> = BroadcastResponse<T>;
}

struct TransClient {
    inner: BoundFendermintClient<HttpClient>,
    broadcast_mode: BroadcastModeWrapper,
}

impl TransClient {
    pub fn new(client: FendermintClient, args: &TransArgs) -> anyhow::Result<Self> {
        let sk = read_secret_key(&args.secret_key)?;
        let addr = to_address(&sk, &args.account_kind)?;
        let chain_id = chainid::from_str_hashed(&args.chain_name)?;
        let mf = SignedMessageFactory::new(sk, addr, args.sequence, chain_id);
        let client = client.bind(mf);
        let client = Self {
            inner: client,
            broadcast_mode: BroadcastModeWrapper(args.broadcast_mode),
        };
        Ok(client)
    }
}

impl BoundClient for TransClient {
    fn message_factory_mut(&mut self) -> &mut SignedMessageFactory {
        self.inner.message_factory_mut()
    }
}

#[async_trait]
impl TxClient<BroadcastModeWrapper> for TransClient {
    async fn perform<F, T>(&self, msg: ChainMessage, f: F) -> anyhow::Result<BroadcastResponse<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
        T: Sync + Send,
    {
        match self.broadcast_mode.0 {
            BroadcastMode::Async => {
                let res = TxClient::<TxAsync>::perform(&self.inner, msg, f).await?;
                Ok(BroadcastResponse::Async(res))
            }
            BroadcastMode::Sync => {
                let res = TxClient::<TxSync>::perform(&self.inner, msg, f).await?;
                Ok(BroadcastResponse::Sync(res))
            }
            BroadcastMode::Commit => {
                let res = TxClient::<TxCommit>::perform(&self.inner, msg, f).await?;
                Ok(BroadcastResponse::Commit(res))
            }
        }
    }
}

fn gas_params(args: &TransArgs) -> GasParams {
    GasParams {
        gas_limit: args.gas_limit,
        gas_fee_cap: args.gas_fee_cap.clone(),
        gas_premium: args.gas_premium.clone(),
    }
}

fn to_address(sk: &SecretKey, kind: &AccountKind) -> anyhow::Result<Address> {
    let pk = sk.public_key().serialize();
    match kind {
        AccountKind::Regular => Ok(Address::new_secp256k1(&pk)?),
        AccountKind::Ethereum => Ok(Address::from(EthAddress::new_secp256k1(&pk)?)),
    }
}
