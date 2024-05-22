// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use crate::error::Error;
use crate::subnet_id::SubnetID;
use crate::{deserialize_human_readable_str, HumanReadable};
use fvm_shared::address::{Address, Protocol};
use serde::ser::Error as SerializeError;
use serde_tuple::{Deserialize_tuple, Serialize_tuple};
use std::{fmt, str::FromStr};

const IPC_SEPARATOR_ADDR: &str = ":";

#[derive(Clone, PartialEq, Eq, Debug, Hash, Serialize_tuple, Deserialize_tuple)]
pub struct IPCAddress {
    subnet_id: SubnetID,
    raw_address: Address,
}

impl IPCAddress {
    /// Generates new IPC address
    pub fn new(sn: &SubnetID, addr: &Address) -> Result<Self, Error> {
        Ok(Self {
            subnet_id: sn.clone(),
            raw_address: *addr,
        })
    }

    /// Returns subnets of a IPC address
    pub fn subnet(&self) -> Result<SubnetID, Error> {
        Ok(self.subnet_id.clone())
    }

    /// Returns the raw address of a IPC address (without subnet context)
    pub fn raw_addr(&self) -> Result<Address, Error> {
        Ok(self.raw_address)
    }

    /// Returns encoded bytes of Address
    #[cfg(feature = "fil-actor")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        Ok(fil_actors_runtime::cbor::serialize(self, "ipc-address")?.to_vec())
    }

    #[cfg(feature = "fil-actor")]
    pub fn from_bytes(bz: &[u8]) -> Result<Self, Error> {
        let i: Self = fil_actors_runtime::cbor::deserialize(
            &fvm_ipld_encoding::RawBytes::new(bz.to_vec()),
            "ipc-address",
        )?;
        Ok(i)
    }

    pub fn to_string(&self) -> Result<String, Error> {
        Ok(format!(
            "{}{}{}",
            self.subnet_id, IPC_SEPARATOR_ADDR, self.raw_address
        ))
    }

    /// Checks if a raw address has a valid Filecoin address protocol
    /// compatible with cross-net messages targetting a contract
    pub fn is_valid_contract_address(addr: &Address) -> bool {
        matches!(addr.protocol(), Protocol::Delegated | Protocol::Actor)
    }

    /// Checks if a raw address has a valid Filecoin address protocol
    /// compatible with cross-net messages targetting a user account
    pub fn is_valid_account_address(addr: &Address) -> bool {
        // we support `Delegated` as a type for a valid account address
        // so we can send funds to eth addresses using cross-net primitives.
        // this may require additional care when executing in FEVM so we don't
        // send funds to a smart contract.
        matches!(
            addr.protocol(),
            Protocol::Delegated | Protocol::BLS | Protocol::Secp256k1 | Protocol::ID
        )
    }
}

impl fmt::Display for IPCAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.subnet_id, IPC_SEPARATOR_ADDR)?;
        write!(f, "{}", self.raw_address)
    }
}

impl FromStr for IPCAddress {
    type Err = Error;

    fn from_str(addr: &str) -> Result<Self, Error> {
        let r: Vec<&str> = addr.split(IPC_SEPARATOR_ADDR).collect();
        if r.len() != 2 {
            Err(Error::InvalidIPCAddr)
        } else {
            Ok(Self {
                raw_address: Address::from_str(r[1])?,
                subnet_id: SubnetID::from_str(r[0])?,
            })
        }
    }
}

impl serde_with::SerializeAs<IPCAddress> for HumanReadable {
    fn serialize_as<S>(address: &IPCAddress, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            address
                .to_string()
                .map_err(|e| {
                    S::Error::custom(format!("cannot convert ipc address to string: {e}"))
                })?
                .serialize(serializer)
        } else {
            address.serialize(serializer)
        }
    }
}

deserialize_human_readable_str!(IPCAddress);

#[cfg(test)]
mod tests {
    use crate::address::IPCAddress;
    use crate::subnet_id::SubnetID;
    use fvm_shared::address::Address;
    use std::str::FromStr;
    use std::vec;

    #[test]
    fn test_ipc_address() {
        let act = Address::new_id(1001);
        let sub_id = SubnetID::new(123, vec![act]);
        let bls = Address::from_str("f3vvmn62lofvhjd2ugzca6sof2j2ubwok6cj4xxbfzz4yuxfkgobpihhd2thlanmsh3w2ptld2gqkn2jvlss4a").unwrap();
        let haddr = IPCAddress::new(&sub_id, &bls).unwrap();

        let str = haddr.to_string().unwrap();

        let blss = IPCAddress::from_str(&str).unwrap();
        assert_eq!(haddr.raw_addr().unwrap(), bls);
        assert_eq!(haddr.subnet().unwrap(), sub_id);
        assert_eq!(haddr, blss);
    }

    #[test]
    fn test_ipc_from_str() {
        let sub_id = SubnetID::new(123, vec![Address::new_id(100)]);
        let addr = IPCAddress::new(&sub_id, &Address::new_id(101)).unwrap();
        let st = addr.to_string().unwrap();
        let addr_out = IPCAddress::from_str(&st).unwrap();
        assert_eq!(addr, addr_out);
        let addr_out = IPCAddress::from_str(&format!("{}", addr)).unwrap();
        assert_eq!(addr, addr_out);
    }

    #[cfg(feature = "fil-actor")]
    #[test]
    fn test_ipc_serialization() {
        let sub_id = SubnetID::new(123, vec![Address::new_id(100)]);
        let addr = IPCAddress::new(&sub_id, &Address::new_id(101)).unwrap();
        let st = addr.to_bytes().unwrap();
        let addr_out = IPCAddress::from_bytes(&st).unwrap();
        assert_eq!(addr, addr_out);
    }
}
