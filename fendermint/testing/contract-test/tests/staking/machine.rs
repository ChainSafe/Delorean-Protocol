// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{cell::RefCell, collections::HashSet, sync::Arc};

use arbitrary::{Arbitrary, Unstructured};
use fendermint_contract_test::ipc::{registry::RegistryCaller, subnet::SubnetCaller};
use fendermint_crypto::{PublicKey, SecretKey};
use fendermint_testing::smt::StateMachine;
use fendermint_vm_actor_interface::{
    eam::EthAddress,
    ipc::{subnet::SubnetActorErrors, subnet_id_to_eth, AbiHash},
};
use fendermint_vm_genesis::{Collateral, Validator, ValidatorKey};
use fendermint_vm_interpreter::fvm::{
    state::{fevm::ContractResult, ipc::GatewayCaller, FvmExecState},
    store::memory::MemoryBlockstore,
};
use fendermint_vm_message::{
    conv::from_fvm::{self, to_eth_tokens},
    signed::sign_secp256k1,
};
use fvm::engine::MultiEngine;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::bigint::Integer;
use fvm_shared::econ::TokenAmount;
use fvm_shared::{address::Address, bigint::BigInt};
use ipc_actors_abis::subnet_actor_checkpointing_facet as checkpointer;
use ipc_api::subnet_id::SubnetID;

use super::{
    choose_amount,
    state::{StakingAccount, StakingState},
};
use fendermint_contract_test::ipc::registry::SubnetConstructorParams;

/// System Under Test for staking.
pub struct StakingSystem {
    /// FVM state initialized with the parent genesis, and a subnet created for the child.
    pub exec_state: RefCell<FvmExecState<MemoryBlockstore>>,
    _gateway: GatewayCaller<MemoryBlockstore>,
    _registry: RegistryCaller<MemoryBlockstore>,
    pub subnet: SubnetCaller<MemoryBlockstore>,
    pub subnet_id: SubnetID,
}

#[derive(Debug)]
pub enum StakingCommand {
    /// Bottom-up checkpoint; confirms all staking operations up to the configuration number.
    Checkpoint {
        block_height: u64,
        block_hash: [u8; 32],
        next_configuration_number: u64,
        signatories: Vec<(EthAddress, SecretKey)>,
    },
    /// Join by as a new validator.
    Join(EthAddress, TokenAmount, PublicKey),
    /// Increase the collateral of an already existing validator.
    Stake(EthAddress, TokenAmount),
    /// Decrease the collateral of a validator.
    Unstake(EthAddress, TokenAmount),
    /// Remove all collateral at once.
    Leave(EthAddress),
    /// Claim released collateral.
    Claim(EthAddress),
}

#[derive(Default)]
pub struct StakingMachine {
    multi_engine: Arc<MultiEngine>,
}

impl StateMachine for StakingMachine {
    type System = StakingSystem;

    type State = StakingState;

    type Command = StakingCommand;

    type Result = ContractResult<(), SubnetActorErrors>;

    fn gen_state(&self, u: &mut Unstructured) -> arbitrary::Result<Self::State> {
        eprintln!("\nNEW STATE");
        StakingState::arbitrary(u)
    }

    fn new_system(&self, state: &Self::State) -> Self::System {
        let rt = tokio::runtime::Runtime::new().expect("create tokio runtime for init");

        let (mut exec_state, _) = rt
            .block_on(fendermint_contract_test::init_exec_state(
                self.multi_engine.clone(),
                state.parent_genesis.clone(),
            ))
            .expect("failed to init parent");

        let gateway = GatewayCaller::default();
        let registry = RegistryCaller::default();

        // Deploy a new subnet based on `state.child_genesis`
        let parent_ipc = state.parent_genesis.ipc.as_ref().unwrap();
        let child_ipc = state.child_genesis.ipc.as_ref().unwrap();

        let (root, route) =
            subnet_id_to_eth(&parent_ipc.gateway.subnet_id).expect("subnet ID is valid");

        // TODO: Need to add field to specify release queue lock time.
        let params = SubnetConstructorParams {
            parent_id: ipc_actors_abis::register_subnet_facet::SubnetID { root, route },
            ipc_gateway_addr: gateway.addr().into(),
            consensus: 0, // TODO: What are the options?
            bottom_up_check_period: child_ipc.gateway.bottom_up_check_period,
            majority_percentage: child_ipc.gateway.majority_percentage,
            active_validators_limit: child_ipc.gateway.active_validators_limit,
            power_scale: state.child_genesis.power_scale,
            min_activation_collateral: to_eth_tokens(&state.min_collateral()).unwrap(),
            min_validators: state.min_validators() as u64,
            permission_mode: 0, // collateral based
            supply_source: ipc_actors_abis::register_subnet_facet::SupplySource {
                kind: 0, // native token
                token_address: ethers::types::Address::zero(),
            },
        };

        eprintln!("\n> PARENT IPC: {parent_ipc:?}");
        eprintln!("\n> CHILD IPC: {child_ipc:?}");
        eprintln!("\n> CREATING SUBNET: {params:?}");

        let subnet_addr = registry
            .new_subnet(&mut exec_state, params)
            .expect("failed to create subnet");

        let subnet_id =
            SubnetID::new_from_parent(&parent_ipc.gateway.subnet_id, subnet_addr.into());

        let subnet = SubnetCaller::new(subnet_addr);

        // Make all the validators join the subnet by putting down collateral according to their power.
        for v in state.child_genesis.validators.iter() {
            let _addr = EthAddress::from(v.public_key.0);
            eprintln!("\n> JOINING SUBNET: addr={_addr} deposit={}", v.power.0);

            subnet
                .join(&mut exec_state, v)
                .expect("failed to join subnet");
        }

        let bootstrapped = subnet
            .bootstrapped(&mut exec_state)
            .expect("failed to call bootstrapped");

        assert!(
            bootstrapped,
            "the genesis joiners should bootstrap the subnet"
        );

        let (next_configuration_number, _) = subnet
            .get_configuration_numbers(&mut exec_state)
            .expect("failed to call config numbers");

        assert_eq!(
            next_configuration_number, 1,
            "after initial joiners configuration should be 1"
        );

        eprintln!("BOOTSTRAPPED");

        StakingSystem {
            exec_state: RefCell::new(exec_state),
            _gateway: gateway,
            _registry: registry,
            subnet,
            subnet_id,
        }
    }

    fn gen_command(
        &self,
        u: &mut Unstructured,
        state: &Self::State,
    ) -> arbitrary::Result<Self::Command> {
        let cmd = u
            .choose(&["checkpoint", "join", "stake", "leave", "claim", "unstake"])
            .unwrap();

        let cmd = match cmd {
            &"checkpoint" => {
                let next_configuration_number = match state.pending_updates.len() {
                    0 => 0, // No change
                    n => {
                        let idx = u.choose_index(n).expect("non-zero");
                        state.pending_updates[idx].configuration_number
                    }
                };

                let ipc_params = state.child_genesis.ipc.clone().unwrap();

                let block_height =
                    state.last_checkpoint_height + ipc_params.gateway.bottom_up_check_period;

                let block_hash = <[u8; 32]>::arbitrary(u)?;

                let majority_percentage = ipc_params.gateway.majority_percentage;
                let collateral = state.active_collateral();
                let collateral = collateral.atto();
                let quorum_threshold =
                    (collateral * majority_percentage).div_ceil(&BigInt::from(100));

                let mut signatories = Vec::new();
                let mut sign_power = BigInt::from(0);

                for (collateral, addr) in state.active_validators() {
                    let a = state.account(addr);
                    signatories.push((*addr, a.secret_key.clone()));
                    sign_power += collateral.0.atto();

                    if sign_power >= quorum_threshold {
                        break;
                    }
                }

                // Technically we cannot build a proper checkpoint here because we don't know the subnet address.
                StakingCommand::Checkpoint {
                    block_height,
                    block_hash,
                    next_configuration_number,
                    signatories,
                }
            }
            &"join" => {
                // Pick any account, doesn't have to be new; the system should handle repeated joins.
                let a = choose_account(u, state)?;
                let b = choose_amount(u, &a.current_balance)?;
                StakingCommand::Join(a.addr, b, a.public_key)
            }
            &"leave" => {
                // Pick any account, doesn't have to be bonded; the system should ignore non-validators and not pay out twice.
                let a = choose_account(u, state)?;
                StakingCommand::Leave(a.addr)
            }
            &"stake" => {
                let a = choose_account(u, state)?;
                // Limit ourselves to the outstanding balance - the user would not be able to send more value to the contract.
                let b = choose_amount(u, &a.current_balance)?;
                StakingCommand::Stake(a.addr, b)
            }
            &"unstake" => {
                let a = choose_account(u, state)?;
                // We can try sending requests to unbond arbitrarily large amounts of collateral - the system should catch any attempt to steal.
                // Only limiting it to be under the initial balance so that it's comparable to what the deposits could have been.
                let b = choose_amount(u, &a.initial_balance)?;
                StakingCommand::Unstake(a.addr, b)
            }
            &"claim" => {
                // Pick any account, even if has nothing to claim; the system should reject those.
                let a = choose_account(u, state)?;
                StakingCommand::Claim(a.addr)
            }
            other => unimplemented!("unknown command: {other}"),
        };
        Ok(cmd)
    }

    fn run_command(&self, system: &mut Self::System, cmd: &Self::Command) -> Self::Result {
        let mut exec_state = system.exec_state.borrow_mut();
        match cmd {
            StakingCommand::Checkpoint {
                block_height,
                block_hash,
                next_configuration_number,
                signatories,
            } => {
                eprintln!(
                    "\n> CMD: CHECKPOINT h={} cn={}",
                    block_height, next_configuration_number
                );

                // Build the checkpoint payload.

                let (root, route) = subnet_id_to_eth(&system.subnet_id).unwrap();

                let checkpoint = checkpointer::BottomUpCheckpoint {
                    subnet_id: checkpointer::SubnetID { root, route },
                    block_height: ethers::types::U256::from(*block_height),
                    block_hash: *block_hash,
                    next_configuration_number: *next_configuration_number,
                    msgs: Vec::new(),
                };
                let checkpoint_hash = checkpoint.clone().abi_hash();

                let mut signatures = Vec::new();

                for (addr, secret_key) in signatories {
                    let signature = sign_secp256k1(secret_key, &checkpoint_hash);
                    let signature = from_fvm::to_eth_signature(&signature, false).unwrap();
                    signatures.push((*addr, signature.into()));
                }

                system
                    .subnet
                    .try_submit_checkpoint(
                        &mut exec_state,
                        checkpoint.clone(),
                        Vec::new(),
                        signatures.clone(),
                    )
                    .expect("failed to call: submit_checkpoint")
            }
            StakingCommand::Join(_addr, value, public_key) => {
                eprintln!("\n> CMD: JOIN addr={_addr} value={value}");
                let validator = Validator {
                    public_key: ValidatorKey(*public_key),
                    power: Collateral(value.clone()),
                };
                system
                    .subnet
                    .try_join(&mut exec_state, &validator)
                    .expect("failed to call: join")
            }
            StakingCommand::Stake(addr, value) => {
                eprintln!("\n> CMD: STAKE addr={addr} value={value}");
                system
                    .subnet
                    .try_stake(&mut exec_state, addr, value)
                    .expect("failed to call: stake")
            }
            StakingCommand::Unstake(addr, value) => {
                eprintln!("\n> CMD: UNSTAKE addr={addr} value={value}");
                system
                    .subnet
                    .try_unstake(&mut exec_state, addr, value)
                    .expect("failed to call: unstake")
            }
            StakingCommand::Leave(addr) => {
                eprintln!("\n> CMD: LEAVE addr={addr}");
                system
                    .subnet
                    .try_leave(&mut exec_state, addr)
                    .expect("failed to call: leave")
            }
            StakingCommand::Claim(addr) => {
                eprintln!("\n> CMD: CLAIM addr={addr}");
                system
                    .subnet
                    .try_claim(&mut exec_state, addr)
                    .expect("failed to call: claim")
            }
        }
    }

    fn check_result(&self, cmd: &Self::Command, pre_state: &Self::State, result: Self::Result) {
        let info = match result {
            Err(ref e) => format!("error: {:?}", e.error),
            Ok(()) => "ok".to_owned(),
        };
        eprintln!("> RESULT: {info}");

        match cmd {
            StakingCommand::Checkpoint { .. } => {
                if !pre_state.can_checkpoint() {
                    result.expect_err("the subnet should be inactive");
                } else {
                    result.expect("checkpoint submission should succeed");
                }
            }
            StakingCommand::Join(eth_addr, value, _) => {
                if value.is_zero() {
                    result.expect_err("should not join with 0 value");
                } else if pre_state.has_staked(eth_addr) {
                    result.expect_err("should not join again");
                } else {
                    result.expect("join should succeed");
                }
            }
            StakingCommand::Stake(addr, value) => {
                if value.is_zero() {
                    result.expect_err("should not stake with 0 value");
                } else if !pre_state.has_staked(addr) {
                    result.expect_err("must call join before stake");
                } else {
                    result.expect("stake should succeed");
                }
            }
            StakingCommand::Unstake(addr, value) => {
                if value.is_zero() {
                    result.expect_err("cannot unstake 0");
                } else if pre_state.total_deposit(addr) <= *value {
                    result.expect_err("tried to unstake too much");
                } else {
                    result.expect("unstake should succeed")
                }
            }
            StakingCommand::Leave(addr) => {
                if !pre_state.has_staked(addr) {
                    result.expect_err("must call join before leave");
                } else {
                    result.expect("leave should succeed");
                }
            }
            StakingCommand::Claim(addr) => {
                if !pre_state.has_claim(addr) {
                    result.expect_err("zero claims should fail");
                } else {
                    result.expect("claim should succeed");
                }
            }
        }
    }

    fn next_state(&self, cmd: &Self::Command, mut state: Self::State) -> Self::State {
        match cmd {
            StakingCommand::Checkpoint {
                next_configuration_number,
                block_height,
                ..
            } => state.checkpoint(*next_configuration_number, *block_height),
            StakingCommand::Join(addr, value, _) => state.join(*addr, value.clone()),
            StakingCommand::Stake(addr, value) => state.stake(*addr, value.clone()),
            StakingCommand::Unstake(addr, value) => state.unstake(*addr, value.clone()),
            StakingCommand::Leave(addr) => state.leave(*addr),
            StakingCommand::Claim(addr) => state.claim(*addr),
        }
        state
    }

    fn check_system(
        &self,
        cmd: &Self::Command,
        post_state: &Self::State,
        post_system: &Self::System,
    ) -> bool {
        // Queries need mutable reference too.
        let mut exec_state = post_system.exec_state.borrow_mut();

        // Check configuration numbers

        let (next_cn, start_cn) = post_system
            .subnet
            .get_configuration_numbers(&mut exec_state)
            .expect("failed to get config numbers");

        assert_eq!(
            next_cn, post_state.next_configuration_number,
            "next configuration number mismatch"
        );

        assert_eq!(
            start_cn,
            post_state.current_configuration.configuration_number + 1,
            "start configuration number mismatch"
        );

        match cmd {
            StakingCommand::Checkpoint { .. } => {
                // Sanity check the reference state while we have no contract to compare with.
                assert!(
                    post_state
                        .accounts
                        .iter()
                        .all(|(_, a)| a.current_balance <= a.initial_balance),
                    "no account goes over initial balance"
                );

                assert!(
                    post_state
                        .current_configuration
                        .collaterals
                        .iter()
                        .all(|(_, p)| !p.0.is_zero()),
                    "all child validators have non-zero collateral"
                );

                // Collect all account info so we can see the ranking, check if there are edge cases.
                let mut obs = Vec::new();

                let active_validators = post_state.active_validators().collect::<Vec<_>>();

                let active_addresses = active_validators
                    .iter()
                    .map(|(_, addr)| addr)
                    .collect::<HashSet<_>>();

                let min_active_collateral = active_validators
                    .last()
                    .map(|(c, _)| c.0.clone())
                    .unwrap_or_default();

                for (addr, a) in post_state.accounts.iter() {
                    // Check balances
                    let sys_balance = get_actor_balance(&mut exec_state, *addr);

                    // Check that we agree on who the active validators are.
                    let sys_collateral = post_system
                        .subnet
                        .confirmed_collateral(&mut exec_state, addr)
                        .expect("failed to get confirmed collateral");

                    let sys_active = post_system
                        .subnet
                        .is_active(&mut exec_state, addr)
                        .expect("failed to call is_waiting");

                    let sys_waiting = post_system
                        .subnet
                        .is_waiting(&mut exec_state, addr)
                        .expect("failed to call is_active");

                    let sys = (sys_balance, sys_collateral, sys_active, sys_waiting);

                    let st_balance = a.current_balance.clone();
                    let st_collateral = post_state.current_configuration.collateral(addr);
                    let st_active = active_addresses.contains(addr);
                    let st_waiting = !st_active && st_collateral.is_positive();

                    let st = (st_balance, st_collateral, st_active, st_waiting);

                    obs.push((addr, sys, st))
                }

                let mut sys_active_cnt = 0;
                let mut st_active_cnt = 0;
                for (addr, (_, sys_coll, sys_active, _), (_, _, st_active, _)) in obs.iter() {
                    if *sys_active || *st_active {
                        eprintln!(
                            "> CONFIRMED addr={:?} collateral={} active=({} vs {})",
                            addr, sys_coll, sys_active, st_active
                        );
                    }
                    if *sys_active {
                        sys_active_cnt += 1;
                    }
                    if *st_active {
                        st_active_cnt += 1;
                    }
                }

                assert!(
                    sys_active_cnt <= post_state.max_validators(),
                    "system over max active"
                );
                assert!(
                    st_active_cnt <= post_state.max_validators(),
                    "state over max active"
                );

                for (
                    addr,
                    (sys_bal, sys_coll, sys_active, sys_waiting),
                    (st_bal, st_coll, st_active, st_waiting),
                ) in obs.iter()
                {
                    assert_eq!(sys_bal, st_bal, "balance mismatch for {addr}");
                    assert_eq!(sys_coll, st_coll, "collateral mismatch for {addr}");

                    if sys_active != st_active && *sys_coll == min_active_collateral {
                        let cnt = obs
                            .iter()
                            .filter(|(_, (_, c, _, _), _)| *c == min_active_collateral)
                            .count();

                        if cnt > 1 {
                            eprintln!(">>> There is a disagreement at the minimum collateral.");
                            eprintln!(">>> Quitting now because the next checkpoint might get invalid signature");
                            return false;
                        }
                    }

                    assert_eq!(sys_active, st_active, "active mismatch for {addr}");
                    assert_eq!(sys_waiting, st_waiting, "waiting mismatch for {addr}");
                }
            }
            StakingCommand::Stake(addr, _)
            | StakingCommand::Unstake(addr, _)
            | StakingCommand::Join(addr, _, _)
            | StakingCommand::Leave(addr)
            | StakingCommand::Claim(addr) => {
                let a = post_state.accounts.get(addr).unwrap();
                assert!(a.current_balance <= a.initial_balance);

                // Check collaterals
                let total = post_system
                    .subnet
                    .total_collateral(&mut exec_state, addr)
                    .expect("failed to get total collateral");
                let confirmed = post_system
                    .subnet
                    .confirmed_collateral(&mut exec_state, addr)
                    .expect("failed to get confirmed collateral");
                assert_eq!(
                    total,
                    post_state.next_configuration.collateral(addr),
                    "total collateral mismatch"
                );
                assert_eq!(
                    confirmed,
                    post_state.current_configuration.collateral(addr),
                    "confirmed collateral mismatch"
                );

                // Check balance
                let balance = get_actor_balance(&mut exec_state, *addr);
                assert_eq!(balance, a.current_balance, "current balance mismatch");
            }
        }
        eprintln!(
            "> LAST UPDATE CONFIG NUMBER: {}",
            post_state.next_configuration.configuration_number
        );

        true
    }
}

fn choose_account<'a>(
    u: &mut Unstructured<'_>,
    state: &'a StakingState,
) -> arbitrary::Result<&'a StakingAccount> {
    let a = u.choose(&state.addrs).expect("accounts not empty");
    let a = state.accounts.get(a).expect("account exists");
    Ok(a)
}

fn get_actor_balance<DB: Blockstore + Clone>(
    exec_state: &mut FvmExecState<DB>,
    addr: EthAddress,
) -> TokenAmount {
    let actor_id = exec_state
        .state_tree_mut()
        .lookup_id(&Address::from(addr))
        .expect("failed to get actor ID")
        .expect("actor exists");

    let actor = exec_state
        .state_tree_mut()
        .get_actor(actor_id)
        .expect("failed to get actor")
        .expect("actor exists");

    actor.balance
}
