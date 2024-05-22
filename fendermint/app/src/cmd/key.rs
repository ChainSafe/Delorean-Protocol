// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, Context};
use fendermint_app_options::key::KeyShowPeerIdArgs;
use fendermint_crypto::{from_b64, to_b64, PublicKey, SecretKey};
use fendermint_vm_actor_interface::eam::EthAddress;
use fvm_shared::address::Address;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use serde_json::json;
use std::path::Path;
use tendermint_config::NodeKey;

use crate::{
    cmd,
    options::key::{
        AddPeer, KeyAddressArgs, KeyArgs, KeyCommands, KeyFromEthArgs, KeyGenArgs, KeyIntoEthArgs,
        KeyIntoTendermintArgs,
    },
};

cmd! {
    KeyArgs(self) {
        match &self.command {
            KeyCommands::Gen(args) => args.exec(()).await,
            KeyCommands::IntoTendermint(args) => args.exec(()).await,
            KeyCommands::AddPeer(args) => args.exec(()).await,
            KeyCommands::Address(args) => args.exec(()).await,
            KeyCommands::FromEth(args) => args.exec(()).await,
            KeyCommands::IntoEth(args) => args.exec(()).await,
            KeyCommands::ShowPeerId(args) => args.exec(()).await,
        }
    }
}

cmd! {
    KeyFromEthArgs(self) {
        let sk = read_secret_key_hex(&self.secret_key)?;
        let pk = sk.public_key();

        export(&self.out_dir, &self.name, "sk", &secret_to_b64(&sk))?;
        export(&self.out_dir, &self.name, "pk", &public_to_b64(&pk))?;

        Ok(())
    }
}

cmd! {
    KeyIntoEthArgs(self) {
        let sk = read_secret_key(&self.secret_key)?;
        let pk = sk.public_key();

        export(&self.out_dir, &self.name, "sk", &hex::encode(sk.serialize()))?;
        export(&self.out_dir, &self.name, "pk", &hex::encode(pk.serialize()))?;
        export(&self.out_dir, &self.name, "addr", &hex::encode(EthAddress::from(pk).0))?;

        Ok(())
    }
}

cmd! {
  KeyGenArgs(self) {
    let mut rng = ChaCha20Rng::from_entropy();
    let sk = SecretKey::random(&mut rng);
    let pk = sk.public_key();

    export(&self.out_dir, &self.name, "sk", &secret_to_b64(&sk))?;
    export(&self.out_dir, &self.name, "pk", &public_to_b64(&pk))?;

    Ok(())
  }
}

cmd! {
  KeyIntoTendermintArgs(self) {
    let sk = read_secret_key(&self.secret_key)?;
    let pk = sk.public_key();
    let vk = tendermint::crypto::default::ecdsa_secp256k1::VerifyingKey::from_sec1_bytes(&pk.serialize())
      .map_err(|e| anyhow!("failed to convert public key: {e}"))?;
    let pub_key = tendermint::PublicKey::Secp256k1(vk);
    let address = tendermint::account::Id::from(pub_key);

    // tendermint-rs doesn't seem to handle Secp256k1 private keys;
    // if it did, we could use tendermint_config::PrivateValidatorKey
    // to encode the data structure. Tendermint should be okay with it
    // though, as long as we match the expected keys in the JSON.
    let priv_validator_key = json! ({
        "address": address,
        "pub_key": pub_key,
        "priv_key": {
            "type": "tendermint/PrivKeySecp256k1",
            "value": secret_to_b64(&sk)
        }
    });
    let json = serde_json::to_string_pretty(&priv_validator_key)?;

    std::fs::write(&self.out, json)?;

    Ok(())
  }
}

cmd! {
    AddPeer(self) {
        let node_key = NodeKey::load_json_file(&self.node_key_file).context("failed to read node key file")?;
        let peer_id = format!("{}@{}", node_key.node_id(), self.network_addr);
        let mut peers = std::fs::read_to_string(&self.local_peers_file).unwrap_or_default();

        if peers.is_empty()  {
            peers.push_str(&peer_id);
        } else {
            peers.push(',');
            peers.push_str(peer_id.as_str());
        }

        std::fs::write(&self.local_peers_file, peers).context("failed to write to the peers file")?;
        Ok(())
  }
}

cmd! {
    KeyAddressArgs(self) {
        let pk = read_public_key(&self.public_key)?;
        let addr = Address::new_secp256k1(&pk.serialize())?;
        println!("{}", addr);
        Ok(())
    }
}

cmd! {
    KeyShowPeerIdArgs(self) {
        let pk = read_public_key(&self.public_key)?;
        // Just using this type because it does the conversion we need.
        let vk = ipc_ipld_resolver::ValidatorKey::from(pk);
        let pk: libp2p::identity::PublicKey = vk.into();
        let id = pk.to_peer_id();
        println!("{}", id);
        Ok(())
    }
}

fn secret_to_b64(sk: &SecretKey) -> String {
    to_b64(sk.serialize().as_ref())
}

fn public_to_b64(pk: &PublicKey) -> String {
    to_b64(&pk.serialize_compressed())
}

fn b64_to_public(b64: &str) -> anyhow::Result<PublicKey> {
    let json = serde_json::json!(b64);
    let pk: PublicKey = serde_json::from_value(json)?;
    Ok(pk)
}

fn b64_to_secret(b64: &str) -> anyhow::Result<SecretKey> {
    let bz = from_b64(b64)?;
    let sk = SecretKey::try_from(bz)?;
    Ok(sk)
}

pub fn read_public_key(public_key: &Path) -> anyhow::Result<PublicKey> {
    let b64 = std::fs::read_to_string(public_key).context("failed to read public key")?;
    let pk = b64_to_public(&b64).context("failed to parse public key")?;
    Ok(pk)
}

pub fn read_secret_key_hex(private_key: &Path) -> anyhow::Result<SecretKey> {
    let hex_str = std::fs::read_to_string(private_key).context("failed to read private key")?;
    let mut hex_str = hex_str.trim();
    if hex_str.starts_with("0x") {
        hex_str = &hex_str[2..];
    }
    let raw_secret = hex::decode(hex_str).context("cannot decode hex private key")?;
    let sk = SecretKey::try_from(raw_secret).context("failed to parse secret key")?;
    Ok(sk)
}

pub fn read_secret_key(secret_key: &Path) -> anyhow::Result<SecretKey> {
    let b64 = std::fs::read_to_string(secret_key).context("failed to read secret key")?;
    let sk = b64_to_secret(&b64).context("failed to parse secret key")?;
    Ok(sk)
}

fn export(output_dir: &Path, name: &str, ext: &str, b64: &str) -> anyhow::Result<()> {
    let output_path = output_dir.join(format!("{name}.{ext}"));
    std::fs::write(output_path, b64)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use fendermint_vm_genesis::ValidatorKey;
    use quickcheck_macros::quickcheck;

    use crate::cmd::key::b64_to_public;

    use super::public_to_b64;

    #[quickcheck]
    fn prop_public_key_deserialize_to_genesis(vk: ValidatorKey) {
        let b64 = public_to_b64(&vk.0);
        let pk = b64_to_public(&b64).unwrap();
        assert_eq!(pk, vk.0)
    }
}
