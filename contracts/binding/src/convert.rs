/// The type conversion for fvm address to evm solidity contracts. We need this convenient macro because
/// the abigen is creating the same struct but under different modules. This save a lot of
/// code.
macro_rules! fvm_address_conversion {
    ($module:ident) => {
        impl TryFrom<fvm_shared::address::Address> for $module::FvmAddress {
            type Error = anyhow::Error;

            fn try_from(value: fvm_shared::address::Address) -> Result<Self, Self::Error> {
                Ok($module::FvmAddress {
                    addr_type: value.protocol() as u8,
                    payload: $crate::convert::addr_payload_to_bytes(value.into_payload())?,
                })
            }
        }

        impl TryFrom<$module::FvmAddress> for fvm_shared::address::Address {
            type Error = anyhow::Error;

            fn try_from(value: $module::FvmAddress) -> Result<Self, Self::Error> {
                let protocol = value.addr_type;
                let addr = $crate::convert::bytes_to_fvm_addr(protocol, &value.payload)?;
                Ok(addr)
            }
        }
    };
}

/// There are common types between the different facets, such as SubnetID. A util macro that handles the common
/// type conversions
macro_rules! common_type_conversion {
    ($module1:ident, $module2:ident) => {
        impl From<$module1::SubnetID> for $module2::SubnetID {
            fn from(value: $module1::SubnetID) -> Self {
                $module2::SubnetID {
                    root: value.root,
                    route: value.route,
                }
            }
        }

        impl From<$module2::SubnetID> for $module1::SubnetID {
            fn from(value: $module2::SubnetID) -> Self {
                $module1::SubnetID {
                    root: value.root,
                    route: value.route,
                }
            }
        }
    };
}

/// Converts a Rust type FVM address into its underlying payload
/// so it can be represented internally in a Solidity contract.
pub(crate) fn addr_payload_to_bytes(
    payload: fvm_shared::address::Payload,
) -> anyhow::Result<ethers::types::Bytes> {
    match payload {
        fvm_shared::address::Payload::Secp256k1(v) => Ok(ethers::types::Bytes::from(v)),
        fvm_shared::address::Payload::Delegated(d) => {
            let addr = d.subaddress();
            let b = ethers::abi::encode(&[ethers::abi::Token::Tuple(vec![
                ethers::abi::Token::Uint(ethers::types::U256::from(d.namespace())),
                ethers::abi::Token::Uint(ethers::types::U256::from(addr.len())),
                ethers::abi::Token::Bytes(addr.to_vec()),
            ])]);
            Ok(ethers::types::Bytes::from(b))
        }
        _ => Err(anyhow::anyhow!("Invalid payload type")),
    }
}

/// It takes the bytes from an FVMAddress represented in Solidity and
/// converts it into the corresponding FVM address Rust type.
pub(crate) fn bytes_to_fvm_addr(
    protocol: u8,
    bytes: &[u8],
) -> anyhow::Result<fvm_shared::address::Address> {
    let addr = match protocol {
        1 => {
            let merged = [[1u8].as_slice(), bytes].concat();
            fvm_shared::address::Address::from_bytes(&merged)?
        }
        4 => {
            let mut data = ethers::abi::decode(
                &[ethers::abi::ParamType::Tuple(vec![
                    ethers::abi::ParamType::Uint(32),
                    ethers::abi::ParamType::Uint(32),
                    ethers::abi::ParamType::Bytes,
                ])],
                bytes,
            )?;

            let mut data = data
                .pop()
                .ok_or_else(|| anyhow::anyhow!("invalid tuple data length"))?
                .into_tuple()
                .ok_or_else(|| anyhow::anyhow!("not tuple"))?;

            let raw_bytes = data
                .pop()
                .ok_or_else(|| anyhow::anyhow!("invalid length, should be 3"))?
                .into_bytes()
                .ok_or_else(|| anyhow::anyhow!("invalid bytes"))?;
            let len = data
                .pop()
                .ok_or_else(|| anyhow::anyhow!("invalid length, should be 3"))?
                .into_uint()
                .ok_or_else(|| anyhow::anyhow!("invalid uint"))?
                .as_u128();
            let namespace = data
                .pop()
                .ok_or_else(|| anyhow::anyhow!("invalid length, should be 3"))?
                .into_uint()
                .ok_or_else(|| anyhow::anyhow!("invalid uint"))?
                .as_u64();

            if len as usize != raw_bytes.len() {
                return Err(anyhow::anyhow!("bytes len not match"));
            }
            fvm_shared::address::Address::new_delegated(namespace, &raw_bytes)?
        }
        _ => return Err(anyhow::anyhow!("address not support now")),
    };
    Ok(addr)
}
