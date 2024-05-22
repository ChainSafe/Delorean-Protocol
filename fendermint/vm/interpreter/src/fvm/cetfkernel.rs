// Copyright 2021-2023 BadBoi Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm::call_manager::CallManager;
use fvm::gas::Gas;
use fvm::kernel::prelude::*;
use fvm::kernel::Context;
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

use ambassador::Delegate;
use cid::Cid;

pub trait CetfKernel: Kernel {
    fn enqueue_tag(&self, tag: &[u8]) -> Result<()>;
}

// our custom kernel extends the filecoin kernel
#[derive(Delegate)]
#[delegate(IpldBlockOps, where = "C: CallManager")]
#[delegate(ActorOps, where = "C: CallManager")]
#[delegate(CryptoOps, where = "C: CallManager")]
#[delegate(DebugOps, where = "C: CallManager")]
#[delegate(EventOps, where = "C: CallManager")]
#[delegate(MessageOps, where = "C: CallManager")]
#[delegate(NetworkOps, where = "C: CallManager")]
#[delegate(RandomnessOps, where = "C: CallManager")]
#[delegate(SelfOps, where = "C: CallManager")]
#[delegate(SendOps<K>, generics = "K", where = "K: CetfKernel")]
#[delegate(UpgradeOps<K>, generics = "K", where = "K: CetfKernel")]
pub struct CetfKernelImpl<C>(pub DefaultKernel<C>);

impl<C> CetfKernel for CetfKernelImpl<C>
where
    C: CallManager,
    CetfKernelImpl<C>: Kernel,
{
    fn enqueue_tag(&self, tag: &[u8]) -> Result<()> {
        // Here we have access to the Kernel structure and can call
        // any of its methods, send messages, etc.

        // We can also run an external program, link to any rust library
        // access the network, etc.

        // TODO: Implement the logic of the syscall here
        println!("Enqueue tag syscall called with tag: {:?}", tag);
        Ok(())
    }
}

impl<C> Kernel for CetfKernelImpl<C>
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
        CetfKernelImpl(DefaultKernel::new(
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

impl<K> SyscallHandler<K> for CetfKernelImpl<K::CallManager>
where
    K: CetfKernel
        + ActorOps
        + SendOps
        + UpgradeOps
        + IpldBlockOps
        + CryptoOps
        + DebugOps
        + EventOps
        + MessageOps
        + NetworkOps
        + RandomnessOps
        + SelfOps,	
{
	fn link_syscalls(linker: &mut Linker<K>) -> anyhow::Result<()> {
        DefaultKernel::<K::CallManager>::link_syscalls(linker)?;
				
        linker.link_syscall("cetf_kernel", "enqueue_tag", enqueue_tag)?;

        Ok(())
    }
}

/// Public exposed interface to allow actors to call the syscall
/// This defines how a tag is read out of the actor memory and passed to the kernel
pub fn enqueue_tag(
    context: fvm::syscalls::Context<'_, impl CetfKernel>,
    tag_off: u32,
    tag_len: u32,
) -> Result<()> {
    let tag = context.memory.try_slice(tag_off, tag_len)?;
    context.kernel.enqueue_tag(tag)
} 
