// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, Context, Result};
use byteorder::{BigEndian, WriteBytesExt};
use cid::Cid;
use fendermint_vm_core::Timestamp;
use fendermint_vm_interpreter::fvm::PowerUpdates;
use fvm_shared::{bigint::Zero, clock::ChainEpoch, econ::TokenAmount, version::NetworkVersion};
use std::{future::Future, sync::Arc};

use fendermint_vm_genesis::Genesis;
use fendermint_vm_interpreter::{
    fvm::{
        bundle::{bundle_path, contracts_path, custom_actors_bundle_path},
        state::{FvmExecState, FvmGenesisState, FvmStateParams, FvmUpdatableParams},
        store::memory::MemoryBlockstore,
        upgrades::UpgradeScheduler,
        FvmApplyRet, FvmGenesisOutput, FvmMessage, FvmMessageInterpreter,
    },
    ExecInterpreter, GenesisInterpreter,
};
use fvm::engine::MultiEngine;

pub mod ipc;

pub async fn init_exec_state(
    multi_engine: Arc<MultiEngine>,
    genesis: Genesis,
) -> anyhow::Result<(FvmExecState<MemoryBlockstore>, FvmGenesisOutput)> {
    let bundle_path = bundle_path();
    let bundle = std::fs::read(&bundle_path)
        .with_context(|| format!("failed to read bundle: {}", bundle_path.to_string_lossy()))?;

    let custom_actors_bundle_path = custom_actors_bundle_path();
    let custom_actors_bundle = std::fs::read(&custom_actors_bundle_path).with_context(|| {
        format!(
            "failed to read custom actors_bundle: {}",
            custom_actors_bundle_path.to_string_lossy()
        )
    })?;

    let store = MemoryBlockstore::new();

    let state = FvmGenesisState::new(store, multi_engine, &bundle, &custom_actors_bundle)
        .await
        .context("failed to create state")?;

    let (client, _) =
        tendermint_rpc::MockClient::new(tendermint_rpc::MockRequestMethodMatcher::default());

    let interpreter = FvmMessageInterpreter::new(
        client,
        None,
        contracts_path(),
        1.05,
        1.05,
        false,
        UpgradeScheduler::new(),
    );

    let (state, out) = interpreter
        .init(state, genesis)
        .await
        .context("failed to create actors")?;

    let state = state
        .into_exec_state()
        .map_err(|_| anyhow!("should be in exec stage"))?;

    Ok((state, out))
}

pub struct Tester<I> {
    interpreter: Arc<I>,
    state_store: Arc<MemoryBlockstore>,
    multi_engine: Arc<MultiEngine>,
    exec_state: Arc<tokio::sync::Mutex<Option<FvmExecState<MemoryBlockstore>>>>,
    state_params: FvmStateParams,
}

impl<I> Tester<I>
where
    I: GenesisInterpreter<
        State = FvmGenesisState<MemoryBlockstore>,
        Genesis = Genesis,
        Output = FvmGenesisOutput,
    >,
    I: ExecInterpreter<
        State = FvmExecState<MemoryBlockstore>,
        Message = FvmMessage,
        BeginOutput = FvmApplyRet,
        DeliverOutput = FvmApplyRet,
        EndOutput = PowerUpdates,
    >,
{
    fn state_store_clone(&self) -> MemoryBlockstore {
        self.state_store.as_ref().clone()
    }

    pub fn new(interpreter: I, state_store: MemoryBlockstore) -> Self {
        Self {
            interpreter: Arc::new(interpreter),
            state_store: Arc::new(state_store),
            multi_engine: Arc::new(MultiEngine::new(1)),
            exec_state: Arc::new(tokio::sync::Mutex::new(None)),
            state_params: FvmStateParams {
                timestamp: Timestamp(0),
                state_root: Cid::default(),
                network_version: NetworkVersion::V21,
                base_fee: TokenAmount::zero(),
                circ_supply: TokenAmount::zero(),
                chain_id: 0,
                power_scale: 0,
                app_version: 0,
            },
        }
    }

    pub async fn init(&mut self, genesis: Genesis) -> anyhow::Result<()> {
        let bundle_path = bundle_path();
        let bundle = std::fs::read(&bundle_path)
            .with_context(|| format!("failed to read bundle: {}", bundle_path.to_string_lossy()))?;

        let custom_actors_bundle_path = custom_actors_bundle_path();
        let custom_actors_bundle =
            std::fs::read(&custom_actors_bundle_path).with_context(|| {
                format!(
                    "failed to read custom actors_bundle: {}",
                    custom_actors_bundle_path.to_string_lossy()
                )
            })?;

        let state = FvmGenesisState::new(
            self.state_store_clone(),
            self.multi_engine.clone(),
            &bundle,
            &custom_actors_bundle,
        )
        .await
        .context("failed to create genesis state")?;

        let (state, out) = self
            .interpreter
            .init(state, genesis)
            .await
            .context("failed to init from genesis")?;

        let state_root = state.commit().context("failed to commit genesis state")?;

        self.state_params = FvmStateParams {
            state_root,
            timestamp: out.timestamp,
            network_version: out.network_version,
            base_fee: out.base_fee,
            circ_supply: out.circ_supply,
            chain_id: out.chain_id.into(),
            power_scale: out.power_scale,
            app_version: 0,
        };

        Ok(())
    }

    /// Take the execution state, update it, put it back, return the output.
    async fn modify_exec_state<T, F, R>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(FvmExecState<MemoryBlockstore>) -> R,
        R: Future<Output = Result<(FvmExecState<MemoryBlockstore>, T)>>,
    {
        let mut guard = self.exec_state.lock().await;
        let state = guard.take().expect("exec state empty");

        let (state, ret) = f(state).await?;

        *guard = Some(state);

        Ok(ret)
    }

    /// Put the execution state during block execution. Has to be empty.
    async fn put_exec_state(&self, state: FvmExecState<MemoryBlockstore>) {
        let mut guard = self.exec_state.lock().await;
        assert!(guard.is_none(), "exec state not empty");
        *guard = Some(state);
    }

    /// Take the execution state during block execution. Has to be non-empty.
    async fn take_exec_state(&self) -> FvmExecState<MemoryBlockstore> {
        let mut guard = self.exec_state.lock().await;
        guard.take().expect("exec state empty")
    }

    pub async fn begin_block(&self, block_height: ChainEpoch) -> Result<()> {
        let mut block_hash: [u8; 32] = [0; 32];
        let _ = block_hash.as_mut().write_i64::<BigEndian>(block_height);

        let db = self.state_store.as_ref().clone();
        let mut state_params = self.state_params.clone();
        state_params.timestamp = Timestamp(block_height as u64);

        let state = FvmExecState::new(db, self.multi_engine.as_ref(), block_height, state_params)
            .context("error creating new state")?
            .with_block_hash(block_hash);

        self.put_exec_state(state).await;

        let _res = self
            .modify_exec_state(|s| self.interpreter.begin(s))
            .await
            .unwrap();

        Ok(())
    }

    pub async fn end_block(&self, _block_height: ChainEpoch) -> Result<()> {
        let _ret = self
            .modify_exec_state(|s| self.interpreter.end(s))
            .await
            .context("end failed")?;

        Ok(())
    }

    pub async fn commit(&mut self) -> Result<()> {
        let exec_state = self.take_exec_state().await;

        let (
            state_root,
            FvmUpdatableParams {
                app_version,
                base_fee,
                circ_supply,
                power_scale,
            },
            _,
        ) = exec_state.commit().context("failed to commit FVM")?;

        self.state_params.state_root = state_root;
        self.state_params.app_version = app_version;
        self.state_params.base_fee = base_fee;
        self.state_params.circ_supply = circ_supply;
        self.state_params.power_scale = power_scale;

        eprintln!("self.state_params: {:?}", self.state_params);

        Ok(())
    }

    pub fn state_params(&self) -> FvmStateParams {
        self.state_params.clone()
    }
}
