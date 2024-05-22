// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

//! Type conversion for IPC Agent struct with solidity contract struct

use crate::address::IPCAddress;
use crate::checkpoint::BottomUpCheckpoint;
use crate::checkpoint::BottomUpMsgBatch;
use crate::cross::{IpcEnvelope, IpcMsgKind};
use crate::staking::StakingChange;
use crate::staking::StakingChangeRequest;
use crate::subnet::SupplySource;
use crate::subnet_id::SubnetID;
use crate::{eth_to_fil_amount, ethers_address_to_fil_address};
use anyhow::anyhow;
use ethers::types::U256;
use fvm_shared::address::{Address, Payload};
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use ipc_actors_abis::{
    gateway_getter_facet, gateway_manager_facet, gateway_messenger_facet, lib_gateway,
    register_subnet_facet, subnet_actor_checkpointing_facet, subnet_actor_diamond,
    subnet_actor_getter_facet, top_down_finality_facet, xnet_messaging_facet,
};

/// The type conversion for IPC structs to evm solidity contracts. We need this convenient macro because
/// the abigen is creating the same struct but under different modules. This save a lot of
/// code.
macro_rules! base_type_conversion {
    ($module:ident) => {
        impl TryFrom<&SubnetID> for $module::SubnetID {
            type Error = anyhow::Error;

            fn try_from(subnet: &SubnetID) -> Result<Self, Self::Error> {
                Ok($module::SubnetID {
                    root: subnet.root_id(),
                    route: subnet_id_to_evm_addresses(subnet)?,
                })
            }
        }

        impl TryFrom<$module::SubnetID> for SubnetID {
            type Error = anyhow::Error;

            fn try_from(value: $module::SubnetID) -> Result<Self, Self::Error> {
                let children = value
                    .route
                    .iter()
                    .map(ethers_address_to_fil_address)
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(SubnetID::new(value.root, children))
            }
        }
    };
}

/// Implement the cross network message types. To use this macro, make sure the $module has already
/// implemented the base types.
macro_rules! cross_msg_types {
    ($module:ident) => {
        impl TryFrom<IPCAddress> for $module::Ipcaddress {
            type Error = anyhow::Error;

            fn try_from(value: IPCAddress) -> Result<Self, Self::Error> {
                Ok($module::Ipcaddress {
                    subnet_id: $module::SubnetID::try_from(&value.subnet()?)?,
                    raw_address: $module::FvmAddress::try_from(value.raw_addr()?)?,
                })
            }
        }

        impl TryFrom<$module::Ipcaddress> for IPCAddress {
            type Error = anyhow::Error;

            fn try_from(value: $module::Ipcaddress) -> Result<Self, Self::Error> {
                let addr = Address::try_from(value.raw_address)?;
                let i = IPCAddress::new(&SubnetID::try_from(value.subnet_id)?, &addr)?;
                Ok(i)
            }
        }

        impl TryFrom<IpcEnvelope> for $module::IpcEnvelope {
            type Error = anyhow::Error;

            fn try_from(value: IpcEnvelope) -> Result<Self, Self::Error> {
                let val = fil_to_eth_amount(&value.value)?;

                let c = $module::IpcEnvelope {
                    kind: value.kind as u8,
                    from: $module::Ipcaddress::try_from(value.from).map_err(|e| {
                        anyhow!("cannot convert `from` ipc address msg due to: {e:}")
                    })?,
                    to: $module::Ipcaddress::try_from(value.to)
                        .map_err(|e| anyhow!("cannot convert `to`` ipc address due to: {e:}"))?,
                    value: val,
                    nonce: value.nonce,
                    message: ethers::core::types::Bytes::from(value.message),
                };
                Ok(c)
            }
        }

        impl TryFrom<$module::IpcEnvelope> for IpcEnvelope {
            type Error = anyhow::Error;

            fn try_from(value: $module::IpcEnvelope) -> Result<Self, Self::Error> {
                let s = IpcEnvelope {
                    from: IPCAddress::try_from(value.from)?,
                    to: IPCAddress::try_from(value.to)?,
                    value: eth_to_fil_amount(&value.value)?,
                    kind: IpcMsgKind::try_from(value.kind)?,
                    message: value.message.to_vec(),
                    nonce: value.nonce,
                };
                Ok(s)
            }
        }
    };
}

/// The type conversion between different bottom up checkpoint definition in ethers and sdk
macro_rules! bottom_up_checkpoint_conversion {
    ($module:ident) => {
        impl TryFrom<BottomUpCheckpoint> for $module::BottomUpCheckpoint {
            type Error = anyhow::Error;

            fn try_from(checkpoint: BottomUpCheckpoint) -> Result<Self, Self::Error> {
                Ok($module::BottomUpCheckpoint {
                    subnet_id: $module::SubnetID::try_from(&checkpoint.subnet_id)?,
                    block_height: ethers::core::types::U256::from(checkpoint.block_height),
                    block_hash: vec_to_bytes32(checkpoint.block_hash)?,
                    next_configuration_number: checkpoint.next_configuration_number,
                    msgs: checkpoint
                        .msgs
                        .into_iter()
                        .map($module::IpcEnvelope::try_from)
                        .collect::<Result<Vec<_>, _>>()?,
                })
            }
        }

        impl TryFrom<$module::BottomUpCheckpoint> for BottomUpCheckpoint {
            type Error = anyhow::Error;

            fn try_from(value: $module::BottomUpCheckpoint) -> Result<Self, Self::Error> {
                Ok(BottomUpCheckpoint {
                    subnet_id: SubnetID::try_from(value.subnet_id)?,
                    block_height: value.block_height.as_u128() as ChainEpoch,
                    block_hash: value.block_hash.to_vec(),
                    next_configuration_number: value.next_configuration_number,
                    msgs: value
                        .msgs
                        .into_iter()
                        .map(IpcEnvelope::try_from)
                        .collect::<Result<Vec<_>, _>>()?,
                })
            }
        }
    };
}

/// The type conversion between different bottom up message batch definition in ethers and sdk
macro_rules! bottom_up_msg_batch_conversion {
    ($module:ident) => {
        impl TryFrom<BottomUpMsgBatch> for $module::BottomUpMsgBatch {
            type Error = anyhow::Error;

            fn try_from(batch: BottomUpMsgBatch) -> Result<Self, Self::Error> {
                Ok($module::BottomUpMsgBatch {
                    subnet_id: $module::SubnetID::try_from(&batch.subnet_id)?,
                    block_height: ethers::core::types::U256::from(batch.block_height),
                    msgs: batch
                        .msgs
                        .into_iter()
                        .map($module::IpcEnvelope::try_from)
                        .collect::<Result<Vec<_>, _>>()?,
                })
            }
        }
    };
}

base_type_conversion!(xnet_messaging_facet);
base_type_conversion!(subnet_actor_getter_facet);
base_type_conversion!(gateway_manager_facet);
base_type_conversion!(subnet_actor_checkpointing_facet);
base_type_conversion!(gateway_getter_facet);
base_type_conversion!(gateway_messenger_facet);
base_type_conversion!(lib_gateway);

cross_msg_types!(gateway_getter_facet);
cross_msg_types!(xnet_messaging_facet);
cross_msg_types!(gateway_messenger_facet);
cross_msg_types!(lib_gateway);
cross_msg_types!(subnet_actor_checkpointing_facet);

bottom_up_checkpoint_conversion!(gateway_getter_facet);
bottom_up_checkpoint_conversion!(subnet_actor_checkpointing_facet);
bottom_up_msg_batch_conversion!(gateway_getter_facet);

impl TryFrom<SupplySource> for subnet_actor_diamond::SupplySource {
    type Error = anyhow::Error;

    fn try_from(value: SupplySource) -> Result<Self, Self::Error> {
        let token_address = if let Some(token_address) = value.token_address {
            payload_to_evm_address(token_address.payload())?
        } else {
            ethers::types::Address::zero()
        };

        Ok(Self {
            kind: value.kind as u8,
            token_address,
        })
    }
}

impl TryFrom<SupplySource> for register_subnet_facet::SupplySource {
    type Error = anyhow::Error;

    fn try_from(value: SupplySource) -> Result<Self, Self::Error> {
        let token_address = if let Some(token_address) = value.token_address {
            payload_to_evm_address(token_address.payload())?
        } else {
            ethers::types::Address::zero()
        };

        Ok(Self {
            kind: value.kind as u8,
            token_address,
        })
    }
}

/// Convert the ipc SubnetID type to a vec of evm addresses. It extracts all the children addresses
/// in the subnet id and turns them as a vec of evm addresses.
pub fn subnet_id_to_evm_addresses(
    subnet: &SubnetID,
) -> anyhow::Result<Vec<ethers::types::Address>> {
    let children = subnet.children();
    children
        .iter()
        .map(|addr| payload_to_evm_address(addr.payload()))
        .collect::<anyhow::Result<_>>()
}

/// Util function to convert Fil address payload to evm address. Only delegated address is supported.
pub fn payload_to_evm_address(payload: &Payload) -> anyhow::Result<ethers::types::Address> {
    match payload {
        Payload::Delegated(delegated) => {
            let slice = delegated.subaddress();
            Ok(ethers::types::Address::from_slice(&slice[0..20]))
        }
        _ => Err(anyhow!("address provided is not delegated")),
    }
}

/// Converts a Fil TokenAmount into an ethers::U256 amount.
pub fn fil_to_eth_amount(amount: &TokenAmount) -> anyhow::Result<U256> {
    let str = amount.atto().to_string();
    Ok(U256::from_dec_str(&str)?)
}

impl TryFrom<StakingChange> for top_down_finality_facet::StakingChange {
    type Error = anyhow::Error;

    fn try_from(value: StakingChange) -> Result<Self, Self::Error> {
        Ok(top_down_finality_facet::StakingChange {
            op: value.op as u8,
            payload: ethers::core::types::Bytes::from(value.payload),
            validator: payload_to_evm_address(value.validator.payload())?,
        })
    }
}

impl TryFrom<StakingChangeRequest> for top_down_finality_facet::StakingChangeRequest {
    type Error = anyhow::Error;

    fn try_from(value: StakingChangeRequest) -> Result<Self, Self::Error> {
        Ok(top_down_finality_facet::StakingChangeRequest {
            change: top_down_finality_facet::StakingChange::try_from(value.change)?,
            configuration_number: value.configuration_number,
        })
    }
}

pub fn vec_to_bytes32(v: Vec<u8>) -> anyhow::Result<[u8; 32]> {
    if v.len() != 32 {
        return Err(anyhow!("invalid length"));
    }

    let mut r = [0u8; 32];
    r.copy_from_slice(&v);

    Ok(r)
}

#[cfg(test)]
mod tests {
    use crate::evm::subnet_id_to_evm_addresses;
    use crate::subnet_id::SubnetID;
    use fvm_shared::address::Address;
    use ipc_types::EthAddress;
    use std::str::FromStr;

    #[test]
    fn test_subnet_id_to_evm_addresses() {
        let eth_addr = EthAddress::from_str("0x0000000000000000000000000000000000000000").unwrap();
        let addr = Address::from(eth_addr);
        let addr2 = Address::from_str("f410ffzyuupbyl2uiucmzr3lu3mtf3luyknthaz4xsrq").unwrap();

        let id = SubnetID::new(0, vec![addr, addr2]);

        let addrs = subnet_id_to_evm_addresses(&id).unwrap();

        let a =
            ethers::types::Address::from_str("0x0000000000000000000000000000000000000000").unwrap();
        let b =
            ethers::types::Address::from_str("0x2e714a3c385ea88a09998ed74db265dae9853667").unwrap();

        assert_eq!(addrs, vec![a, b]);
    }
}
