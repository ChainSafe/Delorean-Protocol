// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! Helper methods to convert between FVM and Ethereum data formats.

use std::str::FromStr;

use anyhow::anyhow;
use anyhow::bail;
use ethers_core::types as et;
use fendermint_crypto::{RecoveryId, Signature};
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_actor_interface::eam::EAM_ACTOR_ID;
use fvm_ipld_encoding::BytesDe;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::chainid::ChainID;
use fvm_shared::crypto::signature::Signature as FvmSignature;
use fvm_shared::crypto::signature::SignatureType;
use fvm_shared::crypto::signature::SECP_SIG_LEN;
use fvm_shared::message::Message;
use fvm_shared::{address::Payload, econ::TokenAmount};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref MAX_U256: BigInt = BigInt::from_str(&et::U256::MAX.to_string()).unwrap();
}

pub fn to_eth_tokens(amount: &TokenAmount) -> anyhow::Result<et::U256> {
    if amount.atto() > &MAX_U256 {
        Err(anyhow!("TokenAmount > U256.MAX"))
    } else {
        let (_sign, bz) = amount.atto().to_bytes_be();
        Ok(et::U256::from_big_endian(&bz))
    }
}

pub fn to_eth_address(addr: &Address) -> anyhow::Result<Option<et::H160>> {
    match addr.payload() {
        Payload::Delegated(d) if d.namespace() == EAM_ACTOR_ID && d.subaddress().len() == 20 => {
            Ok(Some(et::H160::from_slice(d.subaddress())))
        }
        // Deployments should be sent with an empty `to`.
        Payload::ID(EAM_ACTOR_ID) => Ok(None),
        // It should be possible to send to an ethereum account by ID.
        Payload::ID(id) => Ok(Some(et::H160::from_slice(&EthAddress::from_id(*id).0))),
        // The following fit into the type but are not valid ethereum addresses.
        // Return an error so we can prevent tampering with the address when we convert ethereum transactions to FVM messages.
        _ => bail!("not an Ethereum address: {addr}"), // f1, f2, f3 or an invalid delegated address.
    }
}

fn parse_secp256k1(sig: &[u8]) -> anyhow::Result<(RecoveryId, Signature)> {
    if sig.len() != SECP_SIG_LEN {
        return Err(anyhow!("unexpected Secp256k1 length: {}", sig.len()));
    }

    // generate types to recover key from
    let rec_id = RecoveryId::parse(sig[64])?;

    // Signature value without recovery byte
    let mut s = [0u8; 64];
    s.clone_from_slice(&sig[..64]);

    // generate Signature
    let sig = Signature::parse_standard(&s)?;

    Ok((rec_id, sig))
}

/// Convert an FVM signature, which is a normal Secp256k1 signature, to an Ethereum one,
/// where the `v` is optionally shifted by 27 to make it compatible with Solidity.
///
/// In theory we could incorporate the chain ID into it as well, but that hasn't come up.
///
/// Ethers normalizes Ethereum signatures during conversion to RLP.
pub fn to_eth_signature(sig: &FvmSignature, normalized: bool) -> anyhow::Result<et::Signature> {
    let (v, sig) = match sig.sig_type {
        SignatureType::Secp256k1 => parse_secp256k1(&sig.bytes)?,
        other => return Err(anyhow!("unexpected signature type: {other:?}")),
    };

    // By adding 27 to the recovery ID we make this compatible with Ethereum,
    // so that we can verify such signatures in Solidity with e.g. openzeppelin ECDSA.sol
    let shift = if normalized { 0 } else { 27 };

    let sig = et::Signature {
        v: et::U64::from(v.serialize() + shift).as_u64(),
        r: et::U256::from_big_endian(sig.r.b32().as_ref()),
        s: et::U256::from_big_endian(sig.s.b32().as_ref()),
    };

    Ok(sig)
}

/// Turn an FVM `Message` back into an Ethereum transaction request.
pub fn to_eth_transaction_request(
    msg: &Message,
    chain_id: &ChainID,
) -> anyhow::Result<et::Eip1559TransactionRequest> {
    let chain_id: u64 = (*chain_id).into();

    let Message {
        version: _,
        from,
        to,
        sequence,
        value,
        method_num: _,
        params,
        gas_limit,
        gas_fee_cap,
        gas_premium,
    } = msg;

    let data = fvm_ipld_encoding::from_slice::<BytesDe>(params).map(|bz| bz.0)?;

    let mut tx = et::Eip1559TransactionRequest::new()
        .chain_id(chain_id)
        .from(to_eth_address(from)?.unwrap_or_default())
        .nonce(*sequence)
        .gas(*gas_limit)
        .max_fee_per_gas(to_eth_tokens(gas_fee_cap)?)
        .max_priority_fee_per_gas(to_eth_tokens(gas_premium)?)
        .data(et::Bytes::from(data));

    tx.to = to_eth_address(to)?.map(et::NameOrAddress::Address);

    // NOTE: It's impossible to tell if the original Ethereum transaction sent None or Some(0).
    // The ethers deployer sends None, so let's assume that's the useful behavour to match.
    // Luckily the RLP encoding at some point seems to resolve them to the same thing.
    if !value.is_zero() {
        tx.value = Some(to_eth_tokens(value)?);
    }

    Ok(tx)
}

#[cfg(test)]
pub mod tests {

    use std::str::FromStr;

    use ethers::signers::{Signer, Wallet};
    use ethers_core::utils::rlp;
    use ethers_core::{k256::ecdsa::SigningKey, types::transaction::eip2718::TypedTransaction};
    use fendermint_crypto::SecretKey;
    use fendermint_testing::arb::ArbTokenAmount;
    use fendermint_vm_message::signed::SignedMessage;
    use fvm_shared::crypto::signature::Signature;
    use fvm_shared::{bigint::BigInt, chainid::ChainID, econ::TokenAmount};
    use quickcheck_macros::quickcheck;
    use rand::{rngs::StdRng, SeedableRng};

    use crate::conv::{
        from_eth::to_fvm_message,
        tests::{EthMessage, KeyPair},
    };

    use super::{to_eth_signature, to_eth_tokens, to_eth_transaction_request};

    #[quickcheck]
    fn prop_to_eth_tokens(tokens: ArbTokenAmount) -> bool {
        let tokens = tokens.0;
        if let Ok(u256_from_tokens) = to_eth_tokens(&tokens) {
            let tokens_as_str = tokens.atto().to_str_radix(10);
            let u256_from_str = ethers_core::types::U256::from_dec_str(&tokens_as_str).unwrap();
            return u256_from_str == u256_from_tokens;
        }
        true
    }

    #[test]
    fn test_to_eth_tokens() {
        let atto = BigInt::from_str(
            "99191064924191451313862974502415542781658129482631472725645205117646186753315",
        )
        .unwrap();

        let tokens = TokenAmount::from_atto(atto);

        to_eth_tokens(&tokens).unwrap();
    }

    /// Check that converting a signature from FVM to ETH and back preserves it.
    #[quickcheck]
    fn prop_signature(msg: SignedMessage, seed: u64, chain_id: u64) -> Result<(), String> {
        let chain_id = ChainID::from(chain_id);

        let mut rng = StdRng::seed_from_u64(seed);
        let sk = SecretKey::random(&mut rng);

        let msg = SignedMessage::new_secp256k1(msg.into_message(), &sk, &chain_id)
            .map_err(|e| format!("failed to sign: {e}"))?;

        let sig0 = msg.signature();

        let sig1 = to_eth_signature(sig0, true)
            .map_err(|e| format!("failed to convert signature: {e}"))?;

        let sig2 = fvm_shared::crypto::signature::Signature::new_secp256k1(sig1.to_vec());

        if *sig0 != sig2 {
            return Err(format!("signatures don't match: {sig0:?} != {sig2:?}"));
        }
        Ok(())
    }

    #[quickcheck]
    fn prop_to_and_from_eth_transaction(msg: EthMessage, chain_id: u64) {
        let chain_id = ChainID::from(chain_id);
        let msg0 = msg.0;
        let tx = to_eth_transaction_request(&msg0, &chain_id)
            .expect("to_eth_transaction_request failed");
        let msg1 = to_fvm_message(&tx).expect("to_fvm_message failed");

        assert_eq!(msg1, msg0)
    }

    /// Check that decoding a signed ETH transaction and converting to FVM can be verified with the signature produced by a Wallet.
    #[quickcheck]
    fn prop_eth_signature(msg: EthMessage, chain_id: u64, key_pair: KeyPair) {
        // ethers has `to_eip155_v` which would fail with u64 overflow if the chain ID is too big.
        let chain_id = chain_id / 3;

        let chain_id = ChainID::from(chain_id);
        let msg0 = msg.0;
        let tx: TypedTransaction = to_eth_transaction_request(&msg0, &chain_id)
            .expect("to_eth_transaction_request failed")
            .into();

        let wallet: Wallet<SigningKey> = Wallet::from_bytes(key_pair.sk.serialize().as_ref())
            .expect("failed to create wallet")
            .with_chain_id(chain_id);

        let sig = wallet.sign_transaction_sync(&tx).expect("failed to sign");

        let bz = tx.rlp_signed(&sig);
        let rlp = rlp::Rlp::new(bz.as_ref());

        let (tx1, sig) = TypedTransaction::decode_signed(&rlp)
            .expect("failed to decode RLP as signed TypedTransaction");

        let tx1 = tx1.as_eip1559_ref().expect("not an eip1559 transaction");
        let msg1 = to_fvm_message(tx1).expect("to_fvm_message failed");

        let signed = SignedMessage {
            message: msg1,
            signature: Signature::new_secp256k1(sig.to_vec()),
        };

        signed.verify(&chain_id).expect("signature should be valid")
    }
}
