use ethers::providers::{Http, Provider};
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use multihash::MultihashDigest;
use serde::{Deserialize, Serialize};
use std::fmt::Formatter;
use std::{
    fmt::{Debug, Display},
    path::{Path, PathBuf},
};

#[allow(unused_variables, dead_code)] // TODO: Remove once implemented
pub mod docker;
pub mod logging;
pub mod manifest;
pub mod materializer;
pub mod materials;
pub mod testnet;
pub mod validation;

#[cfg(feature = "arb")]
mod arb;

/// An ID identifying a resource within its parent.
#[derive(Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceId(String);

/// Implementing a deserializer which has the logic to sanitise URL-unfriendly characters.
impl<'de> Deserialize<'de> for ResourceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from)
    }
}

impl Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}'", self.0)
    }
}

impl Debug for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<&str> for ResourceId {
    fn from(value: &str) -> Self {
        Self::from(value.to_string())
    }
}

/// Replace the path separator with a different character when reading strings.
impl From<String> for ResourceId {
    fn from(value: String) -> Self {
        Self(value.replace('/', "_"))
    }
}

impl From<&ResourceId> for ResourceId {
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

impl AsRef<str> for ResourceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// A human readable name for a testnet.
pub type TestnetId = ResourceId;

/// A human readable name for an account.
pub type AccountId = ResourceId;

/// A human readable name for a subnet.
pub type SubnetId = ResourceId;

/// A human readable name for a node.
pub type NodeId = ResourceId;

/// A human readable name for a relayer.
pub type RelayerId = ResourceId;

/// The name of a resource consists of its ID and all the IDs of its ancestors
/// concatenated into a URL-like path.
///
/// See <https://cloud.google.com/apis/design/resource_names>
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceName(PathBuf);

impl ResourceName {
    fn join(&self, s: &str) -> Self {
        Self(self.0.join(s))
    }

    fn join_id(&self, id: &ResourceId) -> Self {
        self.join(&id.0)
    }

    pub fn is_prefix_of(&self, other: &ResourceName) -> bool {
        other.0.starts_with(&self.0)
    }

    pub fn path_string(&self) -> String {
        self.0.to_string_lossy().to_string()
    }

    pub fn path(&self) -> &Path {
        self.0.as_path()
    }

    pub fn id(&self) -> ResourceId {
        ResourceId(
            self.0
                .file_name()
                .expect("resource name has file segment")
                .to_string_lossy()
                .to_string(),
        )
    }
}

impl From<&str> for ResourceName {
    fn from(value: &str) -> Self {
        Self(PathBuf::from(value))
    }
}

impl Display for ResourceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}'", self.0.to_string_lossy())
    }
}

impl Debug for ResourceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

pub trait TestnetResource {
    fn testnet(&self) -> TestnetName;
}

macro_rules! resource_name {
    ($name:ident) => {
        #[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(ResourceName);

        impl $name {
            pub fn path(&self) -> &Path {
                &self.0 .0
            }

            pub fn path_string(&self) -> String {
                self.0.path_string()
            }
        }

        impl AsRef<ResourceName> for $name {
            fn as_ref(&self) -> &ResourceName {
                &self.0
            }
        }

        impl AsRef<Path> for $name {
            fn as_ref(&self) -> &Path {
                self.path()
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "{}({})",
                    stringify!($name).trim_end_matches("Name"),
                    self.0
                )
            }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                Display::fmt(&self, f)
            }
        }
    };

    ($name:ident: Testnet) => {
        resource_name!($name);

        impl TestnetResource for $name {
            fn testnet(&self) -> TestnetName {
                TestnetName::from_prefix(&self.0)
            }
        }
    };
}

resource_name!(TestnetName);
resource_name!(AccountName: Testnet);
resource_name!(SubnetName: Testnet);
resource_name!(NodeName: Testnet);
resource_name!(RelayerName: Testnet);
resource_name!(CliName: Testnet);

impl TestnetName {
    pub fn new<T: Into<TestnetId>>(id: T) -> Self {
        // Not including a leadign slash (ie. "/testnets") so that we can join with directory paths.
        Self(ResourceName::from("testnets").join_id(&id.into()))
    }

    pub fn account<T: Into<AccountId>>(&self, id: T) -> AccountName {
        AccountName(self.0.join("accounts").join_id(&id.into()))
    }

    pub fn root(&self) -> SubnetName {
        SubnetName(self.0.join("root"))
    }

    /// Check that the testnet contains a certain resource name, ie. it's a prefix of it.
    pub fn contains<T: AsRef<ResourceName>>(&self, name: T) -> bool {
        self.0.is_prefix_of(name.as_ref())
    }

    /// Assuming correct contstruction of resources, get the testnet prefix.
    fn from_prefix(name: &ResourceName) -> Self {
        name.0
            .components()
            .nth(1)
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .map(Self::new)
            .unwrap_or_else(|| Self(name.clone()))
    }
}

impl SubnetName {
    pub fn subnet<T: Into<SubnetId>>(&self, id: T) -> Self {
        Self(self.0.join("subnets").join_id(&id.into()))
    }

    pub fn node<T: Into<NodeId>>(&self, id: T) -> NodeName {
        NodeName(self.0.join("nodes").join_id(&id.into()))
    }

    pub fn relayer<T: Into<RelayerId>>(&self, id: T) -> RelayerName {
        RelayerName(self.0.join("relayers").join_id(&id.into()))
    }

    pub fn cli(&self, id: &str) -> CliName {
        CliName(self.0.join("cli").join(id))
    }

    /// Check if this is the root subnet, ie. it ends with `root` and it parent is a `testnet`
    pub fn is_root(&self) -> bool {
        self.path().ends_with("root")
            && self
                .path()
                .parent()
                .and_then(|p| p.parent())
                .filter(|p| p.ends_with("testnets"))
                .is_some()
    }

    pub fn parent(&self) -> Option<SubnetName> {
        if self.is_root() {
            None
        } else {
            let path = self
                .path()
                .parent()
                .and_then(|p| p.parent())
                .expect("invalid subnet path");

            Some(Self(ResourceName(path.into())))
        }
    }

    /// All the subnet names from the root to the parent of the subnet,
    /// excluding the subnet itself.
    pub fn ancestors(&self) -> Vec<SubnetName> {
        let mut ss = Vec::new();
        let mut p = self.parent();
        while let Some(s) = p {
            p = s.parent();
            ss.push(s);
        }
        ss.reverse();
        ss
    }

    /// parent->child hop pairs from the root to the current subnet.
    pub fn ancestor_hops(&self, include_self: bool) -> Vec<(SubnetName, SubnetName)> {
        let ss0 = self.ancestors();

        let ss1 = ss0
            .iter()
            .skip(1)
            .chain(std::iter::once(self))
            .cloned()
            .collect::<Vec<_>>();

        let mut hops = ss0.into_iter().zip(ss1).collect::<Vec<_>>();

        if !include_self {
            hops.pop();
        }

        hops
    }

    /// Check that the subnet contains a certain resource name, ie. it's a prefix of it.
    pub fn contains<T: AsRef<ResourceName>>(&self, name: T) -> bool {
        self.0.is_prefix_of(name.as_ref())
    }
}

/// Unique identifier for certain things that we want to keep unique.
#[derive(Clone, Debug, Hash, PartialEq, PartialOrd, Eq, Ord)]
pub struct ResourceHash([u8; 32]);

impl ResourceHash {
    /// Digest some general unique but unwieldy label for a more compact form.
    pub fn digest<T: AsRef<[u8]>>(value: T) -> Self {
        let d = multihash::Code::Blake2b256.digest(value.as_ref());
        let mut bz = [0u8; 32];
        bz.copy_from_slice(d.digest());
        Self(bz)
    }
}

impl Display for ResourceHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

pub trait HasEthApi {
    /// URL of the HTTP endpoint *on the host*, if it's enabled.
    fn ethapi_http_endpoint(&self) -> Option<url::Url>;

    fn ethapi_http_provider(&self) -> anyhow::Result<Option<Provider<Http>>> {
        match self.ethapi_http_endpoint() {
            Some(url) => Ok(Some(Provider::<Http>::try_from(url.to_string())?)),
            None => Ok(None),
        }
    }
}

pub trait HasCometBftApi {
    /// URL of the HTTP endpoint *on the host*.
    fn cometbft_http_endpoint(&self) -> tendermint_rpc::Url;

    fn cometbft_http_provider(&self) -> anyhow::Result<tendermint_rpc::HttpClient> {
        Ok(tendermint_rpc::HttpClient::new(
            self.cometbft_http_endpoint(),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{TestnetName, TestnetResource};

    #[test]
    fn test_path_join() {
        let root = PathBuf::from("/tmp/foo");
        let net = TestnetName::new("bar");
        let acc = net.account("spam");
        let dir = root.join(acc);
        assert_eq!(dir, PathBuf::from("/tmp/foo/testnets/bar/accounts/spam"));
    }

    #[test]
    fn test_subnet_parent() {
        let tn = TestnetName::new("example");
        let rn = tn.root();
        let sn = rn.subnet("foo");
        assert_eq!(rn.parent(), None, "root shouldn't have a parent");
        assert_eq!(sn.parent(), Some(rn), "parent should be the root");
        assert_eq!(sn.testnet(), tn, "testnet is the prefix");
    }

    #[test]
    fn test_subnet_ancestors() {
        let tn = TestnetName::new("example");
        let sn = tn.root().subnet("foo").subnet("bar");
        assert_eq!(sn.ancestors(), vec![tn.root(), tn.root().subnet("foo")]);
    }

    #[test]
    fn test_subnet_ancestor_hops() {
        let tn = TestnetName::new("example");
        let rn = tn.root();
        let foo = rn.subnet("foo");
        let bar = foo.subnet("bar");

        let hops0 = bar.ancestor_hops(false);
        let hops1 = bar.ancestor_hops(true);
        let hops = [(rn, foo.clone()), (foo, bar)];

        assert_eq!(hops0[..], hops[..1]);
        assert_eq!(hops1[..], hops[..]);
    }

    #[test]
    fn test_node_subnet() {
        let tn = TestnetName::new("example");
        let sn = tn.root().subnet("foo");
        let node = sn.node("node-1");

        assert!(sn.contains(&node));
        assert_eq!(node.testnet(), tn, "testnet is the prefix");
    }

    #[test]
    fn test_resource_name_display() {
        let tn = TestnetName::new("display-test");
        assert_eq!(format!("{tn}"), "Testnet('testnets/display-test')");
        assert_eq!(format!("{tn:?}"), "Testnet('testnets/display-test')");
    }
}
