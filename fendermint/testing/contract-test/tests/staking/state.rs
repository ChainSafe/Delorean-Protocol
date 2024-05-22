// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::{BTreeMap, VecDeque};

use arbitrary::Unstructured;
use fendermint_crypto::{PublicKey, SecretKey};
use fendermint_testing::arb::{ArbSubnetAddress, ArbSubnetID, ArbTokenAmount};
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_core::Timestamp;
use fendermint_vm_genesis::ipc::{GatewayParams, IpcParams};
use fendermint_vm_genesis::{
    Account, Actor, ActorMeta, Collateral, Genesis, PermissionMode, SignerAddr, Validator,
    ValidatorKey,
};
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::bigint::Integer;
use fvm_shared::{econ::TokenAmount, version::NetworkVersion};
use ipc_api::subnet_id::SubnetID;
use rand::rngs::StdRng;
use rand::SeedableRng;

use super::choose_amount;

#[derive(Debug, Clone)]
pub enum StakingOp {
    Deposit(TokenAmount),
    Withdraw(TokenAmount),
}

/// The staking message that goes towards the subnet to increase or decrease power.
#[derive(Debug, Clone)]
pub struct StakingUpdate {
    pub configuration_number: u64,
    pub addr: EthAddress,
    pub op: StakingOp,
}

#[derive(Debug, Clone)]
pub struct StakingAccount {
    pub public_key: PublicKey,
    pub secret_key: SecretKey,
    pub addr: EthAddress,
    /// In this test the accounts should never gain more than their initial balance.
    pub initial_balance: TokenAmount,
    /// Balance after the effects of deposits/withdrawals.
    pub current_balance: TokenAmount,
    /// Currently it's not possible to specify the locking period, so all claims are immediately available.
    pub claim_balance: TokenAmount,
}

#[derive(Debug, Clone, Default)]
pub struct StakingDistribution {
    /// The highest configuration number applied.
    pub configuration_number: u64,
    /// Stake for each account that put down some collateral.
    pub collaterals: BTreeMap<EthAddress, Collateral>,
    /// Stakers ordered by collateral in descending order.
    pub ranking: Vec<(Collateral, EthAddress)>,
    /// Total collateral amount, computed because we check it often.
    total_collateral: TokenAmount,
}

impl StakingDistribution {
    /// Sum of all collaterals from active an inactive validators.
    ///
    /// Do not compare this against signature weights because it contains inactive ones!
    pub fn total_collateral(&self) -> TokenAmount {
        self.total_collateral.clone()
    }

    pub fn total_validators(&self) -> usize {
        self.collaterals.len()
    }

    /// Collateral of a validator.
    pub fn collateral(&self, addr: &EthAddress) -> TokenAmount {
        self.collaterals
            .get(addr)
            .map(|c| c.0.clone())
            .unwrap_or_default()
    }

    /// Update the staking distribution. Return the actually applied operation, if any.
    pub fn update(&mut self, update: StakingUpdate) -> Option<StakingOp> {
        self.configuration_number = update.configuration_number;
        let updated = match update.op {
            StakingOp::Deposit(v) => {
                let power = self.collaterals.entry(update.addr).or_default();
                power.0 += v.clone();
                Some((StakingOp::Deposit(v), power.clone()))
            }
            StakingOp::Withdraw(v) => {
                match self.collaterals.entry(update.addr) {
                    std::collections::btree_map::Entry::Occupied(mut e) => {
                        let c = e.get().0.clone();
                        let v = v.min(c.clone());
                        let p = Collateral(c - v.clone());

                        if p.0.is_zero() {
                            e.remove();
                        } else {
                            e.insert(p.clone());
                        };

                        Some((StakingOp::Withdraw(v), p))
                    }
                    std::collections::btree_map::Entry::Vacant(_) => {
                        // Tried to withdraw more than put in.
                        None
                    }
                }
            }
        };

        match updated {
            Some((op, power)) => {
                match op {
                    StakingOp::Deposit(ref v) => self.total_collateral += v.clone(),
                    StakingOp::Withdraw(ref v) => self.total_collateral -= v.clone(),
                }
                self.adjust_rank(update.addr, power);
                Some(op)
            }
            None => None,
        }
    }

    fn adjust_rank(&mut self, addr: EthAddress, power: Collateral) {
        if power.0.is_zero() {
            self.ranking.retain(|(_, a)| *a != addr);
        } else {
            match self.ranking.iter_mut().find(|(_, a)| *a == addr) {
                None => self.ranking.push((power, addr)),
                Some(rank) => rank.0 = power,
            }
            // Sort by collateral descending. Use a stable sort so already sorted items are not affected.
            // Hopefully this works like the sink/swim of the priority queues.
            self.ranking
                .sort_by(|a, b| b.0 .0.atto().cmp(a.0 .0.atto()));
        }
    }
}

/// Reference implementation for staking.
#[derive(Debug, Clone)]
pub struct StakingState {
    /// Accounts with secret key of accounts in case the contract wants to validate signatures.
    pub accounts: BTreeMap<EthAddress, StakingAccount>,
    /// List of account addresses to help pick a random one.
    pub addrs: Vec<EthAddress>,
    /// The parent genesis should include a bunch of accounts we can use to join a subnet.
    pub parent_genesis: Genesis,
    /// The child genesis describes the initial validator set to join the subnet.
    pub child_genesis: Genesis,
    /// Current staking distribution, after the application of checkpoints.
    pub current_configuration: StakingDistribution,
    /// Next staking distribution, applied immediately without involving checkpoints.
    pub next_configuration: StakingDistribution,
    /// Flag indicating whether the minimum collateral has been met.
    pub activated: bool,
    /// Configuration number to be used in the next operation.
    pub next_configuration_number: u64,
    /// Unconfirmed staking operations.
    pub pending_updates: VecDeque<StakingUpdate>,
    /// The block height of the last checkpoint.
    /// The first checkpoint we expect is `0 + bottom_up_checkpoint_period`.
    pub last_checkpoint_height: u64,
}

impl StakingState {
    pub fn new(
        accounts: Vec<StakingAccount>,
        parent_genesis: Genesis,
        child_genesis: Genesis,
    ) -> Self {
        let current_configuration = child_genesis
            .validators
            .iter()
            .map(|v| {
                let addr = EthAddress::new_secp256k1(&v.public_key.0.serialize()).unwrap();
                (addr, v.power.clone())
            })
            .collect::<Vec<_>>();

        let accounts = accounts
            .into_iter()
            .map(|a| (a.addr, a))
            .collect::<BTreeMap<_, _>>();

        let mut addrs: Vec<EthAddress> = accounts.keys().cloned().collect();

        // It's important to sort the addresses so we always pick the same ones given the same seed.
        addrs.sort();

        let mut state = Self {
            accounts,
            addrs,
            parent_genesis,
            child_genesis,
            current_configuration: StakingDistribution::default(),
            next_configuration: StakingDistribution::default(),
            activated: false,
            next_configuration_number: 0,
            pending_updates: VecDeque::new(),
            last_checkpoint_height: 0,
        };

        // Joining one by one so the we test the activation logic
        for (addr, c) in current_configuration {
            state.join(addr, c.0);
        }

        assert!(
            state.activated,
            "subnet should be activated by the child genesis"
        );
        assert_eq!(state.next_configuration_number, 1);

        state
    }

    /// Until the minimum collateral is reached, apply the changes immediately.
    fn update<F: FnOnce(&mut Self) -> StakingUpdate>(&mut self, f: F) {
        let update = f(self);
        let configuration_number = update.configuration_number;

        // Apply on the next configuration immediately.
        let _ = self.next_configuration.update(update.clone());

        // Defer for checkpointing.
        self.pending_updates.push_back(update);

        if !self.activated {
            self.checkpoint(configuration_number, 0);

            let total_collateral = self.current_configuration.total_collateral();
            let total_validators = self.current_configuration.total_validators();

            let min_collateral = self.min_collateral();
            let min_validators = self.min_validators();

            if total_collateral >= min_collateral && total_validators >= min_validators {
                self.activated = true;
                self.next_configuration_number = 1;
            }
        }
    }

    /// Check if checkpoints can be sent to the system.
    pub fn can_checkpoint(&self) -> bool {
        // This is a technical thing of how the the state does transitions, it's all done in the checkpoint method.
        if !self.activated {
            return true;
        }
        // Now the contract expects to be killed explicitly.
        // if self.current_configuration.total_collateral() >= self.min_collateral()
        //     && self.current_configuration.total_validators() >= self.min_collateral()
        // {
        //     return true;
        // }

        // This used to be the case when the collateral fell below a threshold,
        // but now with explicit kill you can always checkpoint until then.
        // return false;

        if self.active_validators().next().is_none() {
            return false;
        }

        true
    }

    /// Apply the changes up to the `next_configuration_number`.
    pub fn checkpoint(&mut self, next_configuration_number: u64, height: u64) {
        // TODO: The contract allows staking operations even after the deactivation of a subnet.
        if self.can_checkpoint() {
            loop {
                if self.pending_updates.is_empty() {
                    break;
                }
                if self.pending_updates[0].configuration_number > next_configuration_number {
                    break;
                }
                let update = self.pending_updates.pop_front().expect("checked non-empty");
                let addr = update.addr;

                if let Some(StakingOp::Withdraw(value)) = self.current_configuration.update(update)
                {
                    self.add_claim(&addr, value);
                }
            }
            self.last_checkpoint_height = height;
        }
    }

    /// Check whether an account has staked before. The stake does not have to be confirmed by a checkpoint.
    pub fn has_staked(&self, addr: &EthAddress) -> bool {
        self.total_deposit(addr).is_positive()
    }

    /// Check whether an account has a non-zero claim balance.
    pub fn has_claim(&self, addr: &EthAddress) -> bool {
        self.account(addr).claim_balance.is_positive()
    }

    /// Total amount staked by a validator.
    pub fn total_deposit(&self, addr: &EthAddress) -> TokenAmount {
        self.next_configuration.collateral(addr)
    }

    /// Maximum number of active validators.
    pub fn max_validators(&self) -> u16 {
        self.child_genesis
            .ipc
            .as_ref()
            .map(|ipc| ipc.gateway.active_validators_limit)
            .unwrap_or_default()
    }

    /// Minimum number of validators required to activate the subnet.
    pub fn min_validators(&self) -> usize {
        // For now just make it so that when all genesis validators join, the subnet is activated.
        self.child_genesis.validators.len()
    }

    /// Minimum collateral required to activate the subnet.
    pub fn min_collateral(&self) -> TokenAmount {
        // For now just make it so that when all genesis validators join, the subnet is activated.
        self.child_genesis
            .validators
            .iter()
            .map(|v| v.power.0.clone())
            .sum()
    }

    /// Top N validators ordered by collateral.
    pub fn active_validators(&self) -> impl Iterator<Item = &(Collateral, EthAddress)> {
        let n = self.max_validators() as usize;
        self.current_configuration.ranking.iter().take(n)
    }

    /// Total collateral of the top N validators.
    ///
    /// This is what we have to achieve quorum over.
    pub fn active_collateral(&self) -> TokenAmount {
        self.active_validators().map(|(c, _)| c.0.clone()).sum()
    }

    /// Get and increment the configuration number.
    fn next_configuration_number(&mut self) -> u64 {
        let n = self.next_configuration_number;
        if self.activated {
            self.next_configuration_number += 1;
        }
        n
    }

    /// Get an account. Panics if it doesn't exist.
    pub fn account(&self, addr: &EthAddress) -> &StakingAccount {
        self.accounts.get(addr).expect("accounts exist")
    }

    /// Get an account. Panics if it doesn't exist.
    fn account_mut(&mut self, addr: &EthAddress) -> &mut StakingAccount {
        self.accounts.get_mut(addr).expect("accounts exist")
    }

    /// Increase the claim balance.
    fn add_claim(&mut self, addr: &EthAddress, value: TokenAmount) {
        let a = self.account_mut(addr);
        eprintln!(
            "> ADD CLAIM addr={} value={} current={}",
            addr, value, a.claim_balance
        );
        a.claim_balance += value;
    }

    /// Increase the current balance.
    fn credit(&mut self, addr: &EthAddress, value: TokenAmount) {
        let a = self.account_mut(addr);
        eprintln!(
            "> CREDIT addr={} value={} current={}",
            addr, value, a.current_balance
        );
        a.current_balance += value;
    }

    /// Decrease the current balance.
    fn debit(&mut self, addr: &EthAddress, value: TokenAmount) {
        let a = self.account_mut(addr);
        eprintln!(
            "> DEBIT addr={} value={} current={}",
            addr, value, a.current_balance
        );
        a.current_balance -= value;
    }

    /// Join with a validator. Repeated joins are allowed.
    ///
    /// Unlike the contract, the model doesn't require metadata here.
    pub fn join(&mut self, addr: EthAddress, value: TokenAmount) {
        if value.is_zero() || self.has_staked(&addr) {
            return;
        }
        self.update(|this| {
            this.debit(&addr, value.clone());

            StakingUpdate {
                configuration_number: {
                    // Add an extra because joining in the model would cause a metadata update as well.
                    this.next_configuration_number();
                    this.next_configuration_number()
                },
                addr,
                op: StakingOp::Deposit(value),
            }
        });
    }

    /// Enqueue a deposit. Must be one of the current validators to succeed, otherwise ignored.
    pub fn stake(&mut self, addr: EthAddress, value: TokenAmount) {
        // Simulate the check the contract does to ensure the metadata has been added before.
        if value.is_zero() || !self.has_staked(&addr) {
            return;
        }
        self.update(|this| {
            this.debit(&addr, value.clone());

            StakingUpdate {
                configuration_number: this.next_configuration_number(),
                addr,
                op: StakingOp::Deposit(value),
            }
        });
    }

    /// Enqueue a withdrawal.
    pub fn unstake(&mut self, addr: EthAddress, value: TokenAmount) {
        if value.is_zero() || self.total_deposit(&addr) <= value {
            return;
        }
        self.update(|this| StakingUpdate {
            configuration_number: this.next_configuration_number(),
            addr,
            op: StakingOp::Withdraw(value),
        });
    }

    /// Enqueue a total withdrawal.
    pub fn leave(&mut self, addr: EthAddress) {
        if !self.has_staked(&addr) {
            return;
        }
        let value = self.total_deposit(&addr);
        self.update(|this| StakingUpdate {
            configuration_number: this.next_configuration_number(),
            addr,
            op: StakingOp::Withdraw(value),
        });
    }

    /// Put released collateral back into the account's current balance.
    pub fn claim(&mut self, addr: EthAddress) {
        let a = self.account_mut(&addr);
        if a.claim_balance.is_zero() {
            return;
        }
        let c = a.claim_balance.clone();
        a.claim_balance = TokenAmount::from_atto(0);
        self.credit(&addr, c);
    }
}

impl arbitrary::Arbitrary<'_> for StakingState {
    fn arbitrary(u: &mut Unstructured<'_>) -> arbitrary::Result<Self> {
        // Limit the maximum number of *child subnet* validators to what the hypothetical consensus algorithm can scale to.
        let num_max_validators = 1 + usize::arbitrary(u)? % 10;
        // Create a number of accounts; it's okay if not everyone can become validators, and also okay if all of them can.
        let num_accounts = 1 + usize::arbitrary(u)? % 20;
        // Choose the size for the initial *child subnet* validator set.
        let num_validators = 1 + usize::arbitrary(u)? % num_accounts.min(num_max_validators);

        // Limit the amount of balance anyone can have so that the sum total of all of them
        // will still be lower than what we can send within Solidity as a value, which is U128.
        let max_balance = BigInt::from(u128::MAX) / num_accounts;

        // Create the desired number of accounts.
        let mut rng = StdRng::seed_from_u64(u64::arbitrary(u)?);
        let mut accounts = Vec::new();
        for _ in 0..num_accounts {
            let sk = SecretKey::random(&mut rng);
            let pk = sk.public_key();
            // All of them need to be ethereum accounts to interact with IPC.
            let addr = EthAddress::new_secp256k1(&pk.serialize()).unwrap();

            // Create with a non-zero balance so we can pick anyone to be a validator and deposit some collateral.
            let initial_balance = ArbTokenAmount::arbitrary(u)?.0;
            let initial_balance = initial_balance.atto();
            let initial_balance = initial_balance.mod_floor(&max_balance);
            let initial_balance =
                TokenAmount::from_atto(initial_balance).max(TokenAmount::from_atto(1).clone());

            // The current balance is the same as the initial balance even if the account becomes
            // one of the validators on the child subnet, because for that they have to join the
            // subnet and that's when their funds are going to be locked up.
            let current_balance = initial_balance.clone();

            accounts.push(StakingAccount {
                public_key: pk,
                secret_key: sk,
                addr,
                initial_balance,
                current_balance,
                claim_balance: TokenAmount::from_atto(0),
            });
        }

        // Accounts on the parent subnet.
        let parent_actors = accounts
            .iter()
            .map(|s| Actor {
                meta: ActorMeta::Account(Account {
                    owner: SignerAddr(Address::from(s.addr)),
                }),
                balance: s.initial_balance.clone(),
            })
            .collect();

        // Select one validator to be the parent validator, it doesn't matter who.
        let parent_validators = vec![Validator {
            public_key: ValidatorKey(accounts[0].public_key),
            // All the power in the parent subnet belongs to this single validator.
            // We are only interested in the staking of the *child subnet*.
            power: Collateral(TokenAmount::from_atto(1)),
        }];

        // Select some of the accounts to be the initial *child subnet* validators.
        let current_configuration = accounts
            .iter()
            .take(num_validators)
            .map(|a| {
                // Choose an initial stake committed to the child subnet.
                let initial_stake = choose_amount(u, &a.initial_balance)?;
                // Make sure it's not zero.
                let initial_stake = initial_stake.max(TokenAmount::from_atto(1));

                Ok(Validator {
                    public_key: ValidatorKey(a.public_key),
                    power: Collateral(initial_stake),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Currently there is a feature flag in the contracts called `FEATURE_SUBNET_DEPTH`
        // that restricts the creation of subnets to be L2 only, so the creator has
        // to live under the root directly.
        let subnet_id = ArbSubnetID::arbitrary(u)?.0;
        let subnet_id = SubnetID::new_root(subnet_id.root_id());

        // IPC of the parent subnet itself - most are not going to be used.
        let parent_ipc = IpcParams {
            gateway: GatewayParams {
                subnet_id,
                bottom_up_check_period: 1 + u.choose_index(100)? as u64,
                majority_percentage: 51 + u8::arbitrary(u)? % 50,
                active_validators_limit: 1 + u.choose_index(100)? as u16,
            },
        };

        // We cannot actually use this value because the real ID will only be
        // apparent once the subnet is deployed.
        let child_subnet_id = SubnetID::new_from_parent(
            &parent_ipc.gateway.subnet_id,
            ArbSubnetAddress::arbitrary(u)?.0,
        );

        let parent_genesis = Genesis {
            chain_name: String::arbitrary(u)?,
            timestamp: Timestamp(u64::arbitrary(u)?),
            network_version: NetworkVersion::V21,
            base_fee: ArbTokenAmount::arbitrary(u)?.0,
            power_scale: *u.choose(&[0, 3]).expect("non empty"),
            validators: parent_validators,
            accounts: parent_actors,
            eam_permission_mode: PermissionMode::Unrestricted,
            ipc: Some(parent_ipc),
        };

        let child_ipc = IpcParams {
            gateway: GatewayParams {
                subnet_id: child_subnet_id,
                bottom_up_check_period: 1 + u.choose_index(100)? as u64,
                majority_percentage: 51 + u8::arbitrary(u)? % 50,
                active_validators_limit: num_max_validators as u16,
            },
        };

        let child_genesis = Genesis {
            chain_name: String::arbitrary(u)?,
            timestamp: Timestamp(u64::arbitrary(u)?),
            network_version: NetworkVersion::V21,
            base_fee: ArbTokenAmount::arbitrary(u)?.0,
            power_scale: *u.choose(&[0, 3]).expect("non empty"),
            validators: current_configuration,
            accounts: Vec::new(),
            eam_permission_mode: PermissionMode::Unrestricted,
            ipc: Some(child_ipc),
        };

        Ok(StakingState::new(accounts, parent_genesis, child_genesis))
    }
}
