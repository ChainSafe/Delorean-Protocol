// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

mod staking;

use anyhow::{Context, Ok};
use async_trait::async_trait;
use ethers::types::U256;
use fendermint_contract_test::Tester;
use fendermint_rpc::response::decode_fevm_return_data;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::str::FromStr;

use ethers::contract::abigen;
use fvm_shared::address::Address;
use fvm_shared::bigint::Zero;
use fvm_shared::econ::TokenAmount;
use fvm_shared::version::NetworkVersion;
use tendermint_rpc::Client;

use fendermint_crypto::SecretKey;
use fendermint_vm_actor_interface::eam;
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_core::Timestamp;
use fendermint_vm_genesis::{Account, Actor, ActorMeta, Genesis, PermissionMode, SignerAddr};
use fendermint_vm_interpreter::fvm::store::memory::MemoryBlockstore;
use fendermint_vm_interpreter::fvm::upgrades::{Upgrade, UpgradeScheduler};
use fendermint_vm_interpreter::fvm::{bundle::contracts_path, FvmMessageInterpreter};

// returns a seeded secret key which is guaranteed to be the same every time
fn my_secret_key() -> SecretKey {
    SecretKey::random(&mut StdRng::seed_from_u64(123))
}

// this test applies a series of upgrades to the state and checks that the upgrades are applied correctly
#[tokio::test]
async fn test_applying_upgrades() {
    use bytes::Bytes;
    use fendermint_rpc::message::{GasParams, MessageFactory};
    use lazy_static::lazy_static;

    lazy_static! {
       /// Default gas params based on the testkit.
       static ref GAS_PARAMS: GasParams = GasParams {
           gas_limit: 10_000_000_000,
           gas_fee_cap: TokenAmount::default(),
           gas_premium: TokenAmount::default(),
       };
       static ref ADDR: Address = Address::new_secp256k1(&my_secret_key().public_key().serialize()).unwrap();
    }

    // this is the contract we want to deploy
    const CONTRACT_HEX: &str = include_str!("../../contracts/SimpleCoin.bin");
    // generate type safe bindings in rust to this contract
    abigen!(SimpleCoin, "../contracts/SimpleCoin.abi");
    // once we deploy this contract, this is the address we expect the contract to be deployed to
    const CONTRACT_ADDRESS: &str = "f410fnz5jdky3zzcj6pejqkomkggw72pcuvkpihz2rwa";
    // the amount we want to send to the contract
    const SEND_BALANCE_AMOUNT: u64 = 1000;
    const CHAIN_NAME: &str = "mychain";

    let mut upgrade_scheduler = UpgradeScheduler::new();
    upgrade_scheduler
        .add(
            Upgrade::new(CHAIN_NAME, 1, Some(1), |state| {
                println!(
                    "[Upgrade at height {}] Deploy simple contract",
                    state.block_height()
                );

                // create a message for deploying the contract
                let mut mf = MessageFactory::new(*ADDR, 1);
                let message = mf
                    .fevm_create(
                        Bytes::from(
                            hex::decode(CONTRACT_HEX)
                                .context("error parsing contract")
                                .unwrap(),
                        ),
                        Bytes::default(),
                        TokenAmount::default(),
                        GAS_PARAMS.clone(),
                    )
                    .unwrap();

                // execute the message
                let (res, _) = state.execute_implicit(message).unwrap();
                assert!(
                    res.msg_receipt.exit_code.is_success(),
                    "{:?}",
                    res.failure_info
                );

                // parse the message receipt data and make sure the contract was deployed to the expected address
                let res = fvm_ipld_encoding::from_slice::<eam::CreateReturn>(
                    &res.msg_receipt.return_data,
                )
                .unwrap();
                assert_eq!(
                    res.delegated_address(),
                    Address::from_str(CONTRACT_ADDRESS).unwrap()
                );

                Ok(())
            })
            .unwrap(),
        )
        .unwrap();

    upgrade_scheduler
        .add(
            Upgrade::new(CHAIN_NAME, 2, None, |state| {
                println!(
                    "[Upgrade at height {}] Sends a balance",
                    state.block_height()
                );

                // build the calldata for the send_coin function
                let (client, _mock) = ethers::providers::Provider::mocked();
                let simple_coin = SimpleCoin::new(EthAddress::from_id(101), client.into());
                let call = simple_coin.send_coin(
                    // the address we are sending the balance to (which is us in this case)
                    EthAddress::from(my_secret_key().public_key()).into(),
                    // the amount we are sending
                    U256::from(SEND_BALANCE_AMOUNT),
                );

                // create a message for sending the balance
                let mut mf = MessageFactory::new(*ADDR, 1);
                let message = mf
                    .fevm_invoke(
                        Address::from_str(CONTRACT_ADDRESS).unwrap(),
                        call.calldata().unwrap().0,
                        TokenAmount::default(),
                        GAS_PARAMS.clone(),
                    )
                    .unwrap();

                // execute the message
                let (res, _) = state.execute_implicit(message).unwrap();
                assert!(
                    res.msg_receipt.exit_code.is_success(),
                    "{:?}",
                    res.failure_info
                );

                Ok(())
            })
            .unwrap(),
        )
        .unwrap();

    upgrade_scheduler
        .add(
            Upgrade::new(CHAIN_NAME, 3, None, |state| {
                println!(
                    "[Upgrade at height {}] Returns a balance",
                    state.block_height()
                );

                // build the calldata for the get_balance function
                let (client, _mock) = ethers::providers::Provider::mocked();
                let simple_coin = SimpleCoin::new(EthAddress::from_id(0), client.into());
                let call =
                    simple_coin.get_balance(EthAddress::from(my_secret_key().public_key()).into());

                let mut mf = MessageFactory::new(*ADDR, 1);
                let message = mf
                    .fevm_invoke(
                        Address::from_str(CONTRACT_ADDRESS).unwrap(),
                        call.calldata().unwrap().0,
                        TokenAmount::default(),
                        GAS_PARAMS.clone(),
                    )
                    .unwrap();

                // execute the message
                let (res, _) = state.execute_implicit(message).unwrap();
                assert!(
                    res.msg_receipt.exit_code.is_success(),
                    "{:?}",
                    res.failure_info
                );

                // parse the message receipt data and make sure the balance we sent in previous upgrade is returned
                let bytes = decode_fevm_return_data(res.msg_receipt.return_data).unwrap();
                let balance = U256::from_big_endian(&bytes);
                assert_eq!(balance, U256::from(SEND_BALANCE_AMOUNT));

                Ok(())
            })
            .unwrap(),
        )
        .unwrap();

    let interpreter: FvmMessageInterpreter<MemoryBlockstore, _> = FvmMessageInterpreter::new(
        NeverCallClient,
        None,
        contracts_path(),
        1.05,
        1.05,
        false,
        upgrade_scheduler,
    );

    let mut tester = Tester::new(interpreter, MemoryBlockstore::new());

    let genesis = Genesis {
        chain_name: CHAIN_NAME.to_string(),
        timestamp: Timestamp(0),
        network_version: NetworkVersion::V21,
        base_fee: TokenAmount::zero(),
        power_scale: 0,
        validators: Vec::new(),
        accounts: vec![Actor {
            meta: ActorMeta::Account(Account {
                owner: SignerAddr(*ADDR),
            }),
            balance: TokenAmount::from_atto(0),
        }],
        eam_permission_mode: PermissionMode::Unrestricted,
        ipc: None,
    };

    tester.init(genesis).await.unwrap();

    // check that the app version is 0
    assert_eq!(tester.state_params().app_version, 0);

    // iterate over all the upgrades
    for block_height in 1..=3 {
        tester.begin_block(block_height).await.unwrap();
        tester.end_block(block_height).await.unwrap();
        tester.commit().await.unwrap();

        // check that the app_version was upgraded to 1
        assert_eq!(tester.state_params().app_version, 1);
    }
}

#[derive(Clone)]
struct NeverCallClient;

#[async_trait]
impl Client for NeverCallClient {
    async fn perform<R>(&self, _request: R) -> Result<R::Output, tendermint_rpc::Error>
    where
        R: tendermint_rpc::SimpleRequest,
    {
        todo!()
    }
}
