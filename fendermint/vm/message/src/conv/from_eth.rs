// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! Helper methods to convert between Ethereum and FVM data formats.

use ethers_core::types::{Eip1559TransactionRequest, NameOrAddress, H160, U256};
use fendermint_vm_actor_interface::{
    eam::{self, EthAddress},
    evm,
};
use fvm_ipld_encoding::{BytesSer, RawBytes};
use fvm_shared::{
    address::Address,
    bigint::{BigInt, Sign},
    econ::TokenAmount,
    message::Message,
};

// https://github.com/filecoin-project/lotus/blob/594c52b96537a8c8728389b446482a2d7ea5617c/chain/types/ethtypes/eth_transactions.go#L152
pub fn to_fvm_message(tx: &Eip1559TransactionRequest) -> anyhow::Result<Message> {
    // FIP-55 says that we should use `InvokeContract` for transfers instead of `METHOD_SEND`,
    // because if we are sending to some Ethereum actor by ID using `METHOD_SEND`, they will
    // get the tokens but the contract might not provide any way of retrieving them.
    // The `Account` actor has been modified to accept any method call, so it will not fail
    // even if it receives tokens using `InvokeContract`.
    let (method_num, to) = match tx.to {
        None => (eam::Method::CreateExternal as u64, eam::EAM_ACTOR_ADDR),
        Some(NameOrAddress::Address(to)) => {
            let to = to_fvm_address(to);
            (evm::Method::InvokeContract as u64, to)
        }
        Some(NameOrAddress::Name(_)) => {
            anyhow::bail!("Turning name to address would require ENS which is not supported.")
        }
    };

    // The `from` of the transaction is inferred from the signature.
    // As long as the client and the server use the same hashing scheme, this should be usable as a delegated address.
    // If none, use the 0x00..00 null ethereum address, which in the node will be replaced with the SYSTEM_ACTOR_ADDR;
    // This is similar to https://github.com/filecoin-project/lotus/blob/master/node/impl/full/eth_utils.go#L124
    let from = to_fvm_address(tx.from.unwrap_or_default());

    // Wrap calldata in IPLD byte format.
    let calldata = tx.data.clone().unwrap_or_default().to_vec();
    let params = RawBytes::serialize(BytesSer(&calldata))?;

    let msg = Message {
        version: 0,
        from,
        to,
        sequence: tx.nonce.unwrap_or_default().as_u64(),
        value: to_fvm_tokens(&tx.value.unwrap_or_default()),
        method_num,
        params,
        gas_limit: tx
            .gas
            .map(|gas| gas.min(U256::from(u64::MAX)).as_u64())
            .unwrap_or_default(),
        gas_fee_cap: to_fvm_tokens(&tx.max_fee_per_gas.unwrap_or_default()),
        gas_premium: to_fvm_tokens(&tx.max_priority_fee_per_gas.unwrap_or_default()),
    };

    Ok(msg)
}

pub fn to_fvm_address(addr: H160) -> Address {
    Address::from(EthAddress(addr.0))
}

pub fn to_fvm_tokens(value: &U256) -> TokenAmount {
    let mut bz = [0u8; 256 / 8];
    value.to_big_endian(&mut bz);
    let atto = BigInt::from_bytes_be(Sign::Plus, &bz);
    TokenAmount::from_atto(atto)
}

#[cfg(test)]
mod tests {

    use ethers_core::{
        types::{transaction::eip2718::TypedTransaction, Bytes, TxHash},
        utils::rlp,
    };
    use fendermint_testing::arb::ArbTokenAmount;
    use fvm_shared::{chainid::ChainID, crypto::signature::Signature};
    use quickcheck_macros::quickcheck;

    use crate::{
        conv::{from_eth::to_fvm_message, from_fvm::to_eth_tokens},
        signed::{DomainHash, SignedMessage},
    };

    use super::to_fvm_tokens;

    #[quickcheck]
    fn prop_to_token_amount(tokens: ArbTokenAmount) -> bool {
        let tokens0 = tokens.0;
        if let Ok(value) = to_eth_tokens(&tokens0) {
            let tokens1 = to_fvm_tokens(&value);
            return tokens0 == tokens1;
        }
        true
    }

    #[test]
    fn test_domain_hash() {
        let expected_hash: TxHash =
            "0x8fe4fd8e1c7c40dceed249c99a553bc218774f611cfefd8a48ede67b8f6e4725"
                .parse()
                .unwrap();

        let raw_tx: Bytes = "0x02f86e87084472af917f2a8080808502540be400948ed26a19f0e0d6708546495611e9a298d9befb598203e880c080a0a37d03d98e50622ec3744ee368565c5e9469852a1d9111197608135928cd2430a010d1575c68602c96c89e9ec30fade44f5844bf34226044d2931afc60b0a8b2de".parse().unwrap();

        let rlp = rlp::Rlp::new(&raw_tx);

        let tx_hash = TxHash::from(ethers_core::utils::keccak256(rlp.as_raw()));
        assert_eq!(tx_hash, expected_hash);

        let (tx0, sig) = TypedTransaction::decode_signed(&rlp).expect("decode signed tx");
        let chain_id: ChainID = tx0.chain_id().unwrap().as_u64().into();

        let msg = SignedMessage {
            message: to_fvm_message(tx0.as_eip1559_ref().unwrap()).expect("to_fvm_message"),
            signature: Signature::new_secp256k1(sig.to_vec()),
        };

        let domain_hash = msg.domain_hash(&chain_id).expect("domain_hash");

        match domain_hash {
            Some(DomainHash::Eth(h)) => assert_eq!(h, tx_hash.0),
            other => panic!("unexpected domain hash: {other:?}"),
        }
    }
}
