// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, bail, Context};
use ethers_core::types as et;
use serde::Deserialize;
use std::{
    cmp::Ord,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    hash::Hash,
    path::{Path, PathBuf},
};

/// Contract source as it appears in dependencies, e.g. `"src/lib/SubnetIDHelper.sol"`, or "Gateway.sol".
/// It is assumed to contain the file extension.
pub type ContractSource = PathBuf;

/// Contract name as it appears in dependencies, e.g. `"SubnetIDHelper"`.
pub type ContractName = String;

pub type ContractSourceAndName = (ContractSource, ContractName);

/// Fully Qualified Name of a contract, e.g. `"src/lib/SubnetIDHelper.sol:SubnetIDHelper"`.
pub type FQN = String;

/// Dependency tree for libraries.
///
/// Using a [BTreeMap] for deterministic ordering.
type DependencyTree<T> = BTreeMap<T, HashSet<T>>;

/// Utility to link bytecode from Hardhat build artifacts.
#[derive(Clone, Debug)]
pub struct Hardhat {
    /// Directory with Hardhat build artifacts, the full-fat JSON files
    /// that contain ABI, bytecode, link references, etc.
    contracts_dir: PathBuf,
}

impl Hardhat {
    pub fn new(contracts_dir: PathBuf) -> Self {
        Self { contracts_dir }
    }

    /// Fully qualified name of a source and contract.
    pub fn fqn(&self, contract_source: &Path, contract_name: &str) -> String {
        format!("{}:{}", contract_source.to_string_lossy(), contract_name)
    }

    /// Read the bytecode of the contract and replace all links in it with library addresses,
    /// similar to how the [hardhat-ethers](https://github.com/NomicFoundation/hardhat/blob/7cc06ab222be8db43265664c68416fdae3030418/packages/hardhat-ethers/src/internal/helpers.ts#L165C42-L165C42)
    /// plugin does it.
    ///
    /// The contract source is expected to be the logical path to a Solidity contract,
    /// including the extension, ie. a [ContractSource].
    pub fn bytecode(
        &self,
        contract_src: impl AsRef<Path>,
        contract_name: &str,
        libraries: &HashMap<FQN, et::Address>,
    ) -> anyhow::Result<Vec<u8>> {
        let artifact = self.artifact(contract_src.as_ref(), contract_name)?;

        // Get the bytecode which is in hex format with placeholders for library references.
        let mut bytecode = artifact.bytecode.object.clone();

        // Replace all library references with their address.
        // Here we differ slightly from the TypeScript version in that we don't return an error
        // for entries in the library address map that we end up not needing, so we can afford
        // to know less about which contract needs which exact references when we call them,
        for (lib_src, lib_name) in artifact.libraries_needed() {
            // References can be given with Fully Qualified Name, or just the contract name,
            // but they must be unique and unambiguous.
            let fqn = self.fqn(&lib_src, &lib_name);

            let lib_addr = match (libraries.get(&fqn), libraries.get(&lib_name)) {
                (None, None) => {
                    bail!("failed to resolve library: {fqn}")
                }
                (Some(_), Some(_)) => bail!("ambiguous library: {fqn}"),
                (Some(addr), None) => addr,
                (None, Some(addr)) => addr,
            };

            let lib_addr = hex::encode(lib_addr.0);

            for pos in artifact.library_positions(&lib_src, &lib_name) {
                let start = 2 + pos.start * 2;
                let end = start + pos.length * 2;
                bytecode.replace_range(start..end, &lib_addr);
            }
        }

        let bytecode = hex::decode(bytecode.trim_start_matches("0x"))
            .context("failed to decode contract from hex")?;

        Ok(bytecode)
    }

    /// Traverse the linked references and return the library contracts to be deployed in topological order.
    ///
    /// The result will include the top contracts as well, and it's up to the caller to filter them out if
    /// they have more complicated deployments including constructors. This is because there can be diamond
    /// facets among them which aren't ABI visible dependencies but should be deployed as libraries.
    pub fn dependencies(
        &self,
        root_contracts: &[(impl AsRef<Path>, &str)],
    ) -> anyhow::Result<Vec<ContractSourceAndName>> {
        let mut deps: DependencyTree<ContractSourceAndName> = Default::default();

        let mut queue = root_contracts
            .iter()
            .map(|(s, c)| (PathBuf::from(s.as_ref()), c.to_string()))
            .collect::<VecDeque<_>>();

        // Construct dependency tree by recursive traversal.
        while let Some(sc) = queue.pop_front() {
            if deps.contains_key(&sc) {
                continue;
            }

            let artifact = self
                .artifact(&sc.0, &sc.1)
                .with_context(|| format!("failed to load dependency artifact: {}", sc.1))?;

            let cds = deps.entry(sc).or_default();

            for (ls, ln) in artifact.libraries_needed() {
                cds.insert((ls.clone(), ln.clone()));
                queue.push_back((ls, ln));
            }
        }

        // Topo-sort the libraries in the order of deployment.
        let sorted = topo_sort(deps)?;

        Ok(sorted)
    }

    /// Concatenate the contracts directory with the expected layout to get
    /// the path to the JSON file of a contract, which is under a directory
    /// named after the Solidity file.
    fn contract_path(&self, contract_src: &Path, contract_name: &str) -> anyhow::Result<PathBuf> {
        // There is currently no example of a Solidity directory containing multiple JSON files,
        // but it possible if there are multiple contracts in the file.

        let base_name = contract_src
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("failed to produce base name for {contract_src:?}"))?;

        let path = self
            .contracts_dir
            .join(base_name)
            .join(format!("{contract_name}.json"));

        Ok(path)
    }

    /// Parse the Hardhat artifact of a contract.
    fn artifact(&self, contract_src: &Path, contract_name: &str) -> anyhow::Result<Artifact> {
        let contract_path = self.contract_path(contract_src, contract_name)?;

        let json = std::fs::read_to_string(&contract_path)
            .with_context(|| format!("failed to read {contract_path:?}"))?;

        let artifact =
            serde_json::from_str::<Artifact>(&json).context("failed to parse Hardhat artifact")?;

        Ok(artifact)
    }
}

#[derive(Deserialize)]
struct Artifact {
    pub bytecode: Bytecode,
}

impl Artifact {
    // Collect the libraries this contract needs.
    pub fn libraries_needed(&self) -> Vec<(ContractSource, ContractName)> {
        self.bytecode
            .link_references
            .iter()
            .flat_map(|(lib_src, links)| {
                links
                    .keys()
                    .map(|lib_name| (lib_src.to_owned(), lib_name.to_owned()))
            })
            .collect()
    }

    pub fn library_positions(
        &self,
        lib_src: &ContractSource,
        lib_name: &ContractName,
    ) -> impl Iterator<Item = &Position> {
        match self
            .bytecode
            .link_references
            .get(lib_src)
            .and_then(|links| links.get(lib_name))
        {
            Some(ps) => ps.iter(),
            None => [].iter(),
        }
    }
}

/// Match the `"bytecode"` entry in the Hardhat build artifact.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bytecode {
    /// Hexadecimal format with placeholders for links.
    pub object: String,
    pub link_references: HashMap<ContractSource, HashMap<ContractName, Vec<Position>>>,
}

/// Indicate where a placeholder appears in the bytecode object.
#[derive(Deserialize)]
struct Position {
    pub start: usize,
    pub length: usize,
}

/// Return elements of a dependency tree in topological order.
fn topo_sort<T>(mut dependency_tree: DependencyTree<T>) -> anyhow::Result<Vec<T>>
where
    T: Eq + PartialEq + Hash + Ord + Clone,
{
    let mut sorted = Vec::new();

    while !dependency_tree.is_empty() {
        let leaf = match dependency_tree.iter().find(|(_, ds)| ds.is_empty()) {
            Some((k, _)) => k.clone(),
            None => bail!("circular reference in the dependencies"),
        };

        dependency_tree.remove(&leaf);

        for (_, ds) in dependency_tree.iter_mut() {
            ds.remove(&leaf);
        }

        sorted.push(leaf);
    }

    Ok(sorted)
}

#[cfg(test)]
mod tests {
    use ethers_core::types as et;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;

    use crate::{topo_sort, DependencyTree};

    use super::Hardhat;

    fn workspace_dir() -> PathBuf {
        let output = std::process::Command::new(env!("CARGO"))
            .arg("locate-project")
            .arg("--workspace")
            .arg("--message-format=plain")
            .output()
            .unwrap()
            .stdout;
        let cargo_path = Path::new(std::str::from_utf8(&output).unwrap().trim());
        cargo_path.parent().unwrap().to_path_buf()
    }

    /// Path to the Solidity contracts, indended to be used in tests.
    fn contracts_path() -> PathBuf {
        let contracts_path = std::env::var("FM_CONTRACTS_DIR").unwrap_or_else(|_| {
            workspace_dir()
                .join("contracts/out")
                .to_string_lossy()
                .into_owned()
        });

        PathBuf::from_str(&contracts_path).expect("malformed contracts path")
    }

    fn test_hardhat() -> Hardhat {
        Hardhat::new(contracts_path())
    }

    // These are all the libraries based on the `scripts/deploy-libraries.ts` in `ipc-solidity-actors`.
    const IPC_DEPS: [&str; 4] = [
        "AccountHelper",
        "SubnetIDHelper",
        "CrossMsgHelper",
        "LibQuorum",
    ];

    #[test]
    fn bytecode_linking() {
        let hardhat = test_hardhat();

        let mut libraries = HashMap::new();

        for lib in IPC_DEPS {
            libraries.insert(lib.to_owned(), et::Address::default());
        }

        // This one requires a subset of above libraries.
        let _bytecode = hardhat
            .bytecode("GatewayManagerFacet.sol", "GatewayManagerFacet", &libraries)
            .unwrap();
    }

    #[test]
    fn bytecode_missing_link() {
        let hardhat = test_hardhat();

        // Not giving any dependency should result in a failure.
        let result = hardhat.bytecode(
            "SubnetActorDiamond.sol",
            "SubnetActorDiamond",
            &Default::default(),
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to resolve library"));
    }

    #[test]
    fn library_dependencies() {
        let hardhat = test_hardhat();

        let root_contracts: Vec<(String, &str)> = vec![
            "GatewayDiamond",
            "GatewayManagerFacet",
            "CheckpointingFacet",
            "TopDownFinalityFacet",
            "XnetMessagingFacet",
            "GatewayGetterFacet",
            "GatewayMessengerFacet",
            "SubnetActorGetterFacet",
            "SubnetActorManagerFacet",
            "SubnetActorRewardFacet",
            "SubnetActorCheckpointingFacet",
            "SubnetActorPauseFacet",
        ]
        .into_iter()
        .map(|c| (format!("{c}.sol"), c))
        .collect();

        // Name our top level contracts and gather all required libraries.
        let mut lib_deps = hardhat
            .dependencies(&root_contracts)
            .expect("failed to compute dependencies");

        // For the sake of testing, let's remove top libraries from the dependency list.
        lib_deps.retain(|(_, d)| !root_contracts.iter().any(|(_, c)| c == d));

        eprintln!("IPC dependencies: {lib_deps:?}");

        assert_eq!(
            lib_deps.len(),
            IPC_DEPS.len(),
            "should discover the same dependencies as expected"
        );

        let mut libs = HashMap::default();

        for (s, c) in lib_deps {
            hardhat.bytecode(&s, &c, &libs).unwrap_or_else(|e| {
                panic!("failed to produce library bytecode in topo order for {c}: {e}")
            });
            // Pretend that we deployed it.
            libs.insert(hardhat.fqn(&s, &c), et::Address::default());
        }

        for (src, name) in root_contracts {
            hardhat
                .bytecode(src, name, &libs)
                .expect("failed to produce contract bytecode in topo order");
        }
    }

    #[test]
    fn topo_sorting() {
        let mut tree: DependencyTree<u8> = Default::default();

        for (k, ds) in [
            (1, vec![]),
            (2, vec![1]),
            (3, vec![1, 2]),
            (4, vec![3]),
            (5, vec![4, 2]),
        ] {
            tree.entry(k).or_default().extend(ds);
        }

        let sorted = topo_sort(tree.clone()).unwrap();

        assert_eq!(sorted.len(), 5);

        for (i, k) in sorted.iter().enumerate() {
            for d in &tree[k] {
                let j = sorted.iter().position(|x| x == d).unwrap();
                assert!(j < i);
            }
        }
    }
}
