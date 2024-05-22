// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail};
use fendermint_app_options::materializer::*;
use fendermint_app_settings::utils::expand_tilde;
use fendermint_materializer::{
    docker::{DockerMaterializer, DropPolicy},
    logging::LoggingMaterializer,
    manifest::Manifest,
    materials::DefaultAccount,
    testnet::Testnet,
    AccountId, TestnetId, TestnetName,
};

use crate::cmd;

use super::key::{read_secret_key, read_secret_key_hex};

cmd! {
  MaterializerArgs(self) {
    let data_dir = expand_tilde(&self.data_dir);
    let dm = || DockerMaterializer::new(&data_dir, self.seed).map(|m| m.with_policy(DropPolicy::PERSISTENT));
    let lm = || dm().map(|m| LoggingMaterializer::new(m, "cli".to_string()));
    match &self.command {
        MaterializerCommands::Validate(args) => args.exec(()).await,
        MaterializerCommands::Setup(args) => args.exec(lm()?).await,
        MaterializerCommands::Remove(args) => args.exec(dm()?).await,
        MaterializerCommands::ImportKey(args) => args.exec(data_dir).await,
    }
  }
}

cmd! {
  MaterializerValidateArgs(self) {
    validate(&self.manifest_file).await
  }
}

cmd! {
  MaterializerSetupArgs(self, m: LoggingMaterializer<DockerMaterializer>) {
    setup(m, &self.manifest_file, self.validate).await
  }
}

cmd! {
  MaterializerRemoveArgs(self, m: DockerMaterializer) {
    remove(m, self.testnet_id.clone()).await
  }
}

cmd! {
  MaterializerImportKeyArgs(self, data_dir: PathBuf) {
    import_key(&data_dir, &self.secret_key, &self.manifest_file, &self.account_id)
  }
}

/// Validate a manifest.
async fn validate(manifest_file: &Path) -> anyhow::Result<()> {
    let (name, manifest) = read_manifest(manifest_file)?;
    manifest.validate(&name).await
}

/// Setup a testnet.
async fn setup(
    mut m: LoggingMaterializer<DockerMaterializer>,
    manifest_file: &Path,
    validate: bool,
) -> anyhow::Result<()> {
    let (name, manifest) = read_manifest(manifest_file)?;

    if validate {
        manifest.validate(&name).await?;
    }

    let _testnet = Testnet::setup(&mut m, &name, &manifest).await?;

    Ok(())
}

/// Remove a testnet.
async fn remove(mut m: DockerMaterializer, id: TestnetId) -> anyhow::Result<()> {
    m.remove(&TestnetName::new(id)).await
}

/// Read a manifest file; use its file name as the testnet name.
fn read_manifest(manifest_file: &Path) -> anyhow::Result<(TestnetName, Manifest)> {
    let testnet_id = manifest_file
        .file_stem()
        .ok_or_else(|| anyhow!("manifest file has no stem"))?
        .to_string_lossy()
        .to_string();

    let name = TestnetName::new(testnet_id);

    let manifest = Manifest::from_file(manifest_file)?;

    Ok((name, manifest))
}

/// Import a secret key as one of the accounts in a manifest.
fn import_key(
    data_dir: &Path,
    secret_key: &Path,
    manifest_file: &Path,
    account_id: &AccountId,
) -> anyhow::Result<()> {
    let (testnet_name, manifest) = read_manifest(manifest_file)?;

    if !manifest.accounts.contains_key(account_id) {
        bail!(
            "account {account_id} cannot be found in the manifest at {}",
            manifest_file.to_string_lossy()
        );
    }

    let sk = read_secret_key(secret_key).or_else(|_| read_secret_key_hex(secret_key))?;

    let _acc = DefaultAccount::create(data_dir, &testnet_name.account(account_id), sk)?;

    Ok(())
}
