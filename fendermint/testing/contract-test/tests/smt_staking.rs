// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//#![allow(unused)]
//! State Machine Test for the Staking contracts.
//!
//! The test simulates random actions validators can take, such as depositing and withdrawing
//! collateral, and executes these actions on the actual Solidity contracts as well as an
//! idealised model, comparing the results and testing that invariants are maintained.
//!
//! It can be executed the following way:
//!
//! ```text
//! cargo test --release -p fendermint_contract_test --test smt_staking
//! ```
use fendermint_testing::{arb::ArbTokenAmount, smt::StateMachine, state_machine_test};

mod staking;

use fendermint_vm_actor_interface::ipc::{abi_hash, AbiHash};
use fendermint_vm_message::conv::from_fvm;
use ipc_actors_abis::subnet_actor_getter_facet;
use staking::machine::StakingMachine;

state_machine_test!(staking, 30000 ms, 65512 bytes, 100 steps, StakingMachine::default());
//state_machine_test!(staking, 0x2924bbae0000ffe8, 100 steps, StakingMachine::default());

/// Test that the way we hash cross messages is the same as Solidity, without having
/// to construct actually executable cross messages.
#[test]
fn prop_cross_msgs_hash() {
    use arbitrary::Arbitrary;
    use subnet_actor_getter_facet as getter;

    // We need an FVM execution state to interact with the contracts.
    let machine = StakingMachine::default();

    fendermint_testing::smt::fixed_size_builder(1024 * 1024)
        .budget_ms(10000) // Need to set a budget otherwise the default is used up by setup.
        .run(|u| {
            let state = machine.gen_state(u)?;
            let system = machine.new_system(&state);

            let mut exec_state = system.exec_state.borrow_mut();

            let mut cross_msgs = Vec::<getter::IpcEnvelope>::new();

            // Generate a few random messages.
            for _ in 0..u.int_in_range(0..=3)? {
                cross_msgs.push(getter::IpcEnvelope {
                    // FIXME: Add different types here?
                    kind: 0,
                    from: getter::Ipcaddress {
                        subnet_id: getter::SubnetID {
                            root: u.arbitrary()?,
                            route: Vec::new(),
                        },
                        raw_address: getter::FvmAddress {
                            addr_type: u.arbitrary()?,
                            payload: <[u8; 20]>::arbitrary(u)?.into(),
                        },
                    },
                    to: getter::Ipcaddress {
                        subnet_id: getter::SubnetID {
                            root: u.arbitrary()?,
                            route: Vec::new(),
                        },
                        raw_address: getter::FvmAddress {
                            addr_type: u.arbitrary()?,
                            payload: <[u8; 20]>::arbitrary(u)?.into(),
                        },
                    },
                    value: from_fvm::to_eth_tokens(&ArbTokenAmount::arbitrary(u)?.0).unwrap(),
                    nonce: u.arbitrary()?,
                    // FIXME: Add arbitrary here?
                    message: Vec::new().into(),
                })
            }

            // Check so we know we did not generate zero length messages all the time.
            fendermint_testing::smt::ensure_has_randomness(u)?;

            // It doesn't seem to actually matter whether we pass these as tuples or arrays.
            let cross_msgs_hash = cross_msgs.clone().abi_hash();
            let cross_msgs_hash_0 = abi_hash(cross_msgs.clone());
            let cross_msgs_hash_1 = abi_hash((cross_msgs.clone(),));
            let cross_msgs_hash_2 = abi_hash(((cross_msgs.clone(),),));

            let hash = system
                .subnet
                .cross_msgs_hash(&mut exec_state, cross_msgs)
                .expect("failed to call cross_msgs_hash");

            assert_eq!(cross_msgs_hash, hash, "impl OK");
            assert_eq!(cross_msgs_hash_0, hash, "array OK");
            assert_eq!(cross_msgs_hash_1, hash, "tuple of array OK");
            assert_ne!(cross_msgs_hash_2, hash, "tuple of tuple of array NOT OK");

            Ok(())
        })
}
