// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use ambassador::Delegate;
use cid::Cid;
use fvm::call_manager::CallManager;
use fvm::gas::Gas;
use fvm::kernel::prelude::*;
use fvm::kernel::Result;
use fvm::kernel::{
    ActorOps, CryptoOps, DebugOps, EventOps, IpldBlockOps, MessageOps, NetworkOps, RandomnessOps,
    SelfOps, SendOps, SyscallHandler, UpgradeOps,
};
use fvm::syscalls::Linker;
use fvm::DefaultKernel;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::randomness::RANDOMNESS_LENGTH;
use fvm_shared::sys::out::network::NetworkContext;
use fvm_shared::sys::out::vm::MessageContext;
use fvm_shared::{address::Address, econ::TokenAmount, ActorID, MethodNum};
use regex::Regex;

#[derive(Delegate)]
#[delegate(IpldBlockOps, where = "C: CallManager")]
#[delegate(ActorOps, where = "C: CallManager")]
#[delegate(CryptoOps, where = "C: CallManager")]
#[delegate(EventOps, where = "C: CallManager")]
#[delegate(MessageOps, where = "C: CallManager")]
#[delegate(NetworkOps, where = "C: CallManager")]
#[delegate(RandomnessOps, where = "C: CallManager")]
#[delegate(SelfOps, where = "C: CallManager")]
#[delegate(SendOps<K>, generics = "K", where = "K: Kernel")]
#[delegate(UpgradeOps<K>, generics = "K", where = "K: Kernel")]
pub struct LoggingKernel<C>(pub DefaultKernel<C>);

impl<C> Kernel for LoggingKernel<C>
where
    C: CallManager,
{
    type CallManager = C;
    type Limiter = <DefaultKernel<C> as Kernel>::Limiter;

    fn into_inner(self) -> (Self::CallManager, BlockRegistry)
    where
        Self: Sized,
    {
        self.0.into_inner()
    }

    fn new(
        mgr: C,
        blocks: BlockRegistry,
        caller: ActorID,
        actor_id: ActorID,
        method: MethodNum,
        value_received: TokenAmount,
        read_only: bool,
    ) -> Self {
        LoggingKernel(DefaultKernel::new(
            mgr,
            blocks,
            caller,
            actor_id,
            method,
            value_received,
            read_only,
        ))
    }

    fn machine(&self) -> &<Self::CallManager as CallManager>::Machine {
        self.0.machine()
    }

    fn limiter_mut(&mut self) -> &mut Self::Limiter {
        self.0.limiter_mut()
    }

    fn gas_available(&self) -> Gas {
        self.0.gas_available()
    }

    fn charge_gas(&self, name: &str, compute: Gas) -> Result<GasTimer> {
        self.0.charge_gas(name, compute)
    }
}

impl<K> SyscallHandler<K> for LoggingKernel<K::CallManager>
where
    K: Kernel
        + ActorOps
        + IpldBlockOps
        + SendOps
        + UpgradeOps
        + CryptoOps
        + DebugOps
        + EventOps
        + MessageOps
        + NetworkOps
        + RandomnessOps
        + SelfOps,
{
    fn link_syscalls(linker: &mut Linker<K>) -> anyhow::Result<()> {
        DefaultKernel::link_syscalls(linker)
    }
}

impl<C> DebugOps for LoggingKernel<C>
where
    C: CallManager,
{
    fn log(&self, msg: String) {
        let (level, actor_name, actor_id, message) = parse_log(&msg).unwrap();
        if level == "INFO" {
            tracing::info!("Actor {}({}) - {}", actor_name, actor_id, message);
        } else if level == "DEBUG" {
            tracing::debug!("Actor {}({}) - {}", actor_name, actor_id, message);
        } else if level == "WARN" {
            tracing::warn!("Actor {}({}) - {}", actor_name, actor_id, message);
        } else if level == "ERROR" {
            tracing::error!("Actor {}({}) - {}", actor_name, actor_id, message);
        }
    }

    fn debug_enabled(&self) -> bool {
        self.0.debug_enabled()
    }

    fn store_artifact(&self, name: &str, data: &[u8]) -> Result<()> {
        self.0.store_artifact(name, data)
    }
}

fn parse_log(log: &str) -> Option<(String, String, i32, String)> {
    let re = Regex::new(r"(?s)\[(.*?)\]<(.*?)::(\d+)> (.*)").unwrap();
    if let Some(captures) = re.captures(log) {
        let first_string = captures.get(1)?.as_str().to_string();
        let second_string = captures.get(2)?.as_str().to_string();
        let number: i32 = captures.get(3)?.as_str().parse().ok()?;
        let fourth_string = captures.get(4)?.as_str().to_string();

        Some((first_string, second_string, number, fourth_string))
    } else {
        None
    }
}
