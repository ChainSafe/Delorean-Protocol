// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, Context};
use fendermint_crypto::PublicKey;
use fvm_shared::address::Address;
use ipc_provider::config::subnet::{EVMSubnet, SubnetConfig};
use ipc_provider::IpcProvider;
use std::path::PathBuf;

use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_core::{chainid, Timestamp};
use fendermint_vm_genesis::{
    ipc, Account, Actor, ActorMeta, Collateral, Genesis, Multisig, PermissionMode, SignerAddr,
    Validator, ValidatorKey,
};

use crate::cmd;
use crate::options::genesis::*;

use super::key::read_public_key;

cmd! {
  GenesisArgs(self) {
    let genesis_file = self.genesis_file.clone();
    match &self.command {
        GenesisCommands::New(args) => args.exec(genesis_file).await,
        GenesisCommands::AddAccount(args) => args.exec(genesis_file).await,
        GenesisCommands::AddMultisig(args) => args.exec(genesis_file).await,
        GenesisCommands::AddValidator(args) => args.exec(genesis_file).await,
        GenesisCommands::IntoTendermint(args) => args.exec(genesis_file).await,
        GenesisCommands::SetEamPermissions(args) => args.exec(genesis_file).await,
        GenesisCommands::Ipc { command } => command.exec(genesis_file).await,
    }
  }
}

cmd! {
  GenesisNewArgs(self, genesis_file: PathBuf) {
    let genesis = Genesis {
      timestamp: Timestamp(self.timestamp),
      chain_name: self.chain_name.clone(),
      network_version: self.network_version,
      base_fee: self.base_fee.clone(),
      power_scale: self.power_scale,
      validators: Vec::new(),
      accounts: Vec::new(),
      eam_permission_mode: PermissionMode::Unrestricted,
      ipc: None,
    };

    let json = serde_json::to_string_pretty(&genesis)?;
    std::fs::write(genesis_file, json)?;

    Ok(())
  }
}

cmd! {
  GenesisAddAccountArgs(self, genesis_file: PathBuf) {
    add_account(&genesis_file, self)
  }
}

cmd! {
  GenesisAddMultisigArgs(self, genesis_file: PathBuf) {
    add_multisig(&genesis_file, self)
  }
}

cmd! {
  GenesisAddValidatorArgs(self, genesis_file: PathBuf) {
    add_validator(&genesis_file, self)
  }
}

cmd! {
  GenesisIntoTendermintArgs(self, genesis_file: PathBuf) {
    into_tendermint(&genesis_file, self)
  }
}

cmd! {
  GenesisSetEAMPermissionsArgs(self, genesis_file: PathBuf) {
    set_eam_permissions(&genesis_file, self)
  }
}

cmd! {
  GenesisIpcCommands(self, genesis_file: PathBuf) {
    match self {
        GenesisIpcCommands::Gateway(args) =>
            set_ipc_gateway(&genesis_file, args),
        GenesisIpcCommands::FromParent(args) =>
            new_genesis_from_parent(&genesis_file, args).await,
    }
  }
}

fn add_account(genesis_file: &PathBuf, args: &GenesisAddAccountArgs) -> anyhow::Result<()> {
    update_genesis(genesis_file, |mut genesis| {
        let pk = read_public_key(&args.public_key)?;
        let pk = pk.serialize();
        let addr = match args.kind {
            AccountKind::Regular => Address::new_secp256k1(&pk)?,
            AccountKind::Ethereum => Address::from(EthAddress::new_secp256k1(&pk)?),
        };
        let meta = ActorMeta::Account(Account {
            owner: SignerAddr(addr),
        });
        if genesis.accounts.iter().any(|a| a.meta == meta) {
            return Err(anyhow!("account already exists in the genesis file"));
        }
        let actor = Actor {
            meta,
            balance: args.balance.clone(),
        };
        genesis.accounts.push(actor);
        Ok(genesis)
    })
}

fn add_multisig(genesis_file: &PathBuf, args: &GenesisAddMultisigArgs) -> anyhow::Result<()> {
    update_genesis(genesis_file, |mut genesis| {
        let mut signers = Vec::new();
        for p in &args.public_key {
            let pk = read_public_key(p)?;
            let addr = SignerAddr(Address::new_secp256k1(&pk.serialize())?);
            if signers.contains(&addr) {
                return Err(anyhow!("duplicated signer: {}", p.to_string_lossy()));
            }
            signers.push(addr);
        }

        if signers.is_empty() {
            return Err(anyhow!("there needs to be at least one signer"));
        }
        if signers.len() < args.threshold as usize {
            return Err(anyhow!("threshold cannot be higher than number of signers"));
        }
        if args.threshold == 0 {
            return Err(anyhow!("threshold must be positive"));
        }

        let ms = Multisig {
            signers,
            threshold: args.threshold,
            vesting_duration: args.vesting_duration,
            vesting_start: args.vesting_start,
        };

        let actor = Actor {
            meta: ActorMeta::Multisig(ms),
            balance: args.balance.clone(),
        };

        genesis.accounts.push(actor);

        Ok(genesis)
    })
}

fn add_validator(genesis_file: &PathBuf, args: &GenesisAddValidatorArgs) -> anyhow::Result<()> {
    update_genesis(genesis_file, |mut genesis| {
        let pk = read_public_key(&args.public_key)?;
        let vk = ValidatorKey(pk);
        if genesis.validators.iter().any(|v| v.public_key == vk) {
            return Err(anyhow!("account already exists in the genesis file"));
        }
        let validator = Validator {
            public_key: vk,
            power: Collateral(args.power.clone()),
        };
        genesis.validators.push(validator);
        Ok(genesis)
    })
}

fn read_genesis(genesis_file: &PathBuf) -> anyhow::Result<Genesis> {
    let json = std::fs::read_to_string(genesis_file).context("failed to read genesis")?;
    let genesis = serde_json::from_str::<Genesis>(&json).context("failed to parse genesis")?;
    Ok(genesis)
}

fn update_genesis<F>(genesis_file: &PathBuf, f: F) -> anyhow::Result<()>
where
    F: FnOnce(Genesis) -> anyhow::Result<Genesis>,
{
    let genesis = read_genesis(genesis_file)?;
    let genesis = f(genesis)?;
    let json = serde_json::to_string_pretty(&genesis)?;
    std::fs::write(genesis_file, json)?;
    Ok(())
}

fn set_eam_permissions(
    genesis_file: &PathBuf,
    args: &GenesisSetEAMPermissionsArgs,
) -> anyhow::Result<()> {
    update_genesis(genesis_file, |mut genesis| {
        genesis.eam_permission_mode = match args.mode.to_lowercase().as_str() {
            "unrestricted" => PermissionMode::Unrestricted,
            "allowlist" => {
                let addresses = args.addresses.clone();
                PermissionMode::AllowList { addresses }
            }
            _ => return Err(anyhow!("unknown eam permisison mode")),
        };
        Ok(genesis)
    })
}

fn into_tendermint(genesis_file: &PathBuf, args: &GenesisIntoTendermintArgs) -> anyhow::Result<()> {
    let genesis = read_genesis(genesis_file)?;
    let genesis_json = serde_json::to_value(&genesis)?;

    let chain_id: u64 = chainid::from_str_hashed(&genesis.chain_name)?.into();
    let chain_id = chain_id.to_string();

    let tmg = tendermint::Genesis {
        genesis_time: tendermint::time::Time::from_unix_timestamp(genesis.timestamp.as_secs(), 0)?,
        chain_id: tendermint::chain::Id::try_from(chain_id)?,
        // CometBFT chains typically start from height 1. It doesn't seem to matter if we set this to 0,
        // the `init_chain` ABCI method will still receive 1.
        initial_height: 1,
        // Values are based on the default produced by `tendermint init`
        consensus_params: tendermint::consensus::Params {
            block: tendermint::block::Size {
                max_bytes: args.block_max_bytes,
                max_gas: -1,
                time_iota_ms: tendermint::block::Size::default_time_iota_ms(),
            },
            evidence: tendermint::evidence::Params {
                max_age_num_blocks: 100000,
                max_age_duration: tendermint::evidence::Duration(std::time::Duration::from_nanos(
                    172800000000000,
                )),
                max_bytes: 1048576,
            },
            validator: tendermint::consensus::params::ValidatorParams {
                pub_key_types: vec![tendermint::public_key::Algorithm::Secp256k1],
            },
            version: Some(tendermint::consensus::params::VersionParams { app: 0 }),
        },
        // Validators will be returnd from `init_chain`.
        validators: Vec::new(),
        // Hopefully leaving this empty will skip validation,
        // otherwise we have to run the genesis in memory here and now.
        app_hash: tendermint::AppHash::default(),
        app_state: genesis_json,
    };
    let tmg_json = serde_json::to_string_pretty(&tmg)?;
    std::fs::write(&args.out, tmg_json)?;
    Ok(())
}

fn set_ipc_gateway(genesis_file: &PathBuf, args: &GenesisIpcGatewayArgs) -> anyhow::Result<()> {
    update_genesis(genesis_file, |mut genesis| {
        let gateway_params = ipc::GatewayParams {
            subnet_id: args.subnet_id.clone(),
            bottom_up_check_period: args.bottom_up_check_period,
            majority_percentage: args.majority_percentage,
            active_validators_limit: args.active_validators_limit,
        };

        let ipc_params = match genesis.ipc {
            Some(mut ipc) => {
                ipc.gateway = gateway_params;
                ipc
            }
            None => ipc::IpcParams {
                gateway: gateway_params,
            },
        };

        genesis.ipc = Some(ipc_params);

        Ok(genesis)
    })
}

async fn new_genesis_from_parent(
    genesis_file: &PathBuf,
    args: &GenesisFromParentArgs,
) -> anyhow::Result<()> {
    // provider with the parent.
    let parent_provider = IpcProvider::new_with_subnet(
        None,
        ipc_provider::config::Subnet {
            id: args
                .subnet_id
                .parent()
                .ok_or_else(|| anyhow!("subnet is not a child"))?,
            config: SubnetConfig::Fevm(EVMSubnet {
                provider_http: args.parent_endpoint.clone(),
                provider_timeout: None,
                auth_token: args.parent_auth_token.clone(),
                registry_addr: args.parent_registry,
                gateway_addr: args.parent_gateway,
            }),
        },
    )?;

    let genesis_info = parent_provider.get_genesis_info(&args.subnet_id).await?;

    // get gateway genesis
    let ipc_params = ipc::IpcParams {
        gateway: ipc::GatewayParams {
            subnet_id: args.subnet_id.clone(),
            bottom_up_check_period: genesis_info.bottom_up_checkpoint_period,
            majority_percentage: genesis_info.majority_percentage,
            active_validators_limit: genesis_info.active_validators_limit,
        },
    };
    let mut genesis = Genesis {
        // We set the genesis epoch as the genesis timestamp so it can be
        // generated deterministically by all participants
        // genesis_epoch should be a positive number, we can afford panicking
        // here if this is not the case.
        timestamp: Timestamp(genesis_info.genesis_epoch.try_into().unwrap()),
        chain_name: args.subnet_id.to_string(),
        network_version: args.network_version,
        base_fee: args.base_fee.clone(),
        power_scale: args.power_scale,
        validators: Vec::new(),
        accounts: Vec::new(),
        eam_permission_mode: PermissionMode::Unrestricted,
        ipc: Some(ipc_params),
    };

    for v in genesis_info.validators {
        let pk = PublicKey::parse_slice(&v.metadata, None)?;
        genesis.validators.push(Validator {
            public_key: ValidatorKey(pk),
            power: Collateral(v.weight),
        })
    }

    for (a, b) in genesis_info.genesis_balances {
        let meta = ActorMeta::Account(Account {
            owner: SignerAddr(a),
        });
        let actor = Actor {
            meta,
            balance: b.clone(),
        };
        genesis.accounts.push(actor);
    }

    let json = serde_json::to_string_pretty(&genesis)?;
    std::fs::write(genesis_file, json)?;

    Ok(())
}
