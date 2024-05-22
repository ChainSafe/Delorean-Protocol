// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Path to a golden file.
fn path(prefix: &str, name: &str, ext: &str) -> String {
    // All files will have the same name but different extension.
    // They should be under `fendermint/vm/message/golden`.
    let path = Path::new("golden").join(prefix).join(name);
    format!("{}.{}", path.display(), ext)
}

/// Read the contents of an existing golden file, or create it by turning `fallback` into string first.
fn read_or_create<T>(
    prefix: &str,
    name: &str,
    ext: &str,
    fallback: &T,
    to_string: fn(&T) -> String,
) -> String {
    let p = path(prefix, name, ext);
    let p = Path::new(&p);

    if !p.exists() {
        if let Some(p) = p.parent() {
            std::fs::create_dir_all(p).expect("failed to create golden directory");
        }
        let s = to_string(fallback);
        let mut f = File::create(p)
            .unwrap_or_else(|e| panic!("Cannot create golden file at {:?}: {}", p, e));
        f.write_all(s.as_bytes()).unwrap();
    }

    let mut f =
        File::open(p).unwrap_or_else(|e| panic!("Cannot open golden file at {:?}: {}", p, e));

    let mut s = String::new();
    f.read_to_string(&mut s).expect("Cannot read golden file.");
    s.trim_end().to_owned()
}

/// Check that a golden file we created earlier can still be read by the current model by
/// comparing to a debug string (which should at least be readable enough to show what changed).
///
/// If the golden file doesn't exist, create one now.
fn test_txt<T>(
    prefix: &str,
    name: &str,
    arb_data: fn(g: &mut quickcheck::Gen) -> T,
    ext: &str,
    to_string: fn(&T) -> String,
    from_string: fn(&String) -> Result<T, String>,
) -> T
where
    T: Serialize + DeserializeOwned + Debug,
{
    // We may not need this, but it shouldn't be too expensive to generate.
    let mut g = quickcheck::Gen::new(10);
    let data0 = arb_data(&mut g);

    // Debug string of a wrapper.
    let to_debug = |w: &T| format!("{:?}", w);

    let repr = read_or_create(prefix, name, ext, &data0, to_string);

    let data1: T = from_string(&repr)
        .unwrap_or_else(|e| panic!("Cannot deserialize {prefix}/{name}.{ext}: {e}"));

    // Use the deserialised data as fallback for the debug string, so if the txt doesn't exist, it's created
    // from what we just read back.
    let txt = read_or_create(prefix, name, "txt", &data1, to_debug);

    // This will fail if either the CBOR or the Debug format changes.
    // At that point we should either know that it's a legitimate regression because we changed the model,
    // or catch it as an unexpected regression, indicating that we made some backwards incompatible change.
    assert_eq!(to_debug(&data1), txt.trim_end());

    data1
}

/// Test CBOR golden file.
///
/// Note that the CBOR files will be encoded as hexadecimal strings.
/// To view them in something like https://cbor.dev/ you can use for example `xxd`:
///
/// ```text
/// cat example.cbor | xxd -r -p > example.cbor.bin
/// ```
pub fn test_cbor_txt<T: Serialize + DeserializeOwned + Debug>(
    prefix: &str,
    name: &str,
    arb_data: fn(g: &mut quickcheck::Gen) -> T,
) -> T {
    test_txt(
        prefix,
        name,
        arb_data,
        "cbor",
        |d| {
            let bz = fvm_ipld_encoding::to_vec(d).expect("failed to serialize");
            hex::encode(bz)
        },
        |s| {
            let bz = hex::decode(s).map_err(|e| format!("faled to decode hex: {e}"))?;
            fvm_ipld_encoding::from_slice(&bz).map_err(|e| format!("failed to decode CBOR: {e}"))
        },
    )
}

/// Same as [`test_cbor_txt`] but with JSON.
pub fn test_json_txt<T: Serialize + DeserializeOwned + Debug>(
    prefix: &str,
    name: &str,
    arb_data: fn(g: &mut quickcheck::Gen) -> T,
) -> T {
    test_txt(
        prefix,
        name,
        arb_data,
        "json",
        |d| serde_json::to_string_pretty(d).expect("failed to serialize"),
        |s| serde_json::from_str(s).map_err(|e| format!("failed to decode JSON: {e}")),
    )
}

/// Same as [`test_json_txt`] but with YAML.
pub fn test_yaml_txt<T: Serialize + DeserializeOwned + Debug>(
    prefix: &str,
    name: &str,
    arb_data: fn(g: &mut quickcheck::Gen) -> T,
) -> T {
    test_txt(
        prefix,
        name,
        arb_data,
        "yaml",
        |d| serde_yaml::to_string(d).expect("failed to serialize"),
        |s| serde_yaml::from_str(s).map_err(|e| format!("failed to decode YAML: {e}")),
    )
}

/// Same as [`test_json_txt`] but with TOML.
pub fn test_toml_txt<T: Serialize + DeserializeOwned + Debug>(
    prefix: &str,
    name: &str,
    arb_data: fn(g: &mut quickcheck::Gen) -> T,
) -> T {
    test_txt(
        prefix,
        name,
        arb_data,
        "toml",
        |d| toml::to_string(d).expect("failed to serialize"),
        |s| toml::from_str(s).map_err(|e| format!("failed to decode TOML: {e}")),
    )
}

/// Test that the CID of something we deserialized from CBOR matches what we saved earlier,
/// ie. that we produce the same CID, which is important if it's the basis of signing.
pub fn test_cid<T: Debug>(prefix: &str, name: &str, data: T, cid: fn(&T) -> Cid) {
    let exp_cid = hex::encode(cid(&data).to_bytes());
    let got_cid = read_or_create(prefix, name, "cid", &exp_cid, |d| d.to_owned());
    assert_eq!(got_cid, exp_cid)
}

/// Create a test which calls [`test_cbor_txt`].
///
/// # Example
///
/// ```ignore
///        golden_cbor! { "query/response", actor_state, |g| {
///            ActorState::arbitrary(g)
///        }}
/// ```
#[macro_export]
macro_rules! golden_cbor {
    ($prefix:literal, $name:ident, $gen:expr) => {
        #[test]
        fn $name() {
            let label = stringify!($name);
            $crate::golden::test_cbor_txt($prefix, &label, $gen);
        }
    };
}

/// Create a test which calls [`test_json_txt`].
///
/// # Example
///
/// ```ignore
///        golden_json! { "genesis", genesis, Genesis::arbitrary}
/// ```
#[macro_export]
macro_rules! golden_json {
    ($prefix:literal, $name:ident, $gen:expr) => {
        #[test]
        fn $name() {
            let label = stringify!($name);
            $crate::golden::test_json_txt($prefix, &label, $gen);
        }
    };
}

/// Create a test which calls [`test_yaml_txt`].
///
/// # Example
///
/// ```ignore
///        golden_yaml! { "genesis", genesis, Genesis::arbitrary}
/// ```
#[macro_export]
macro_rules! golden_yaml {
    ($prefix:literal, $name:ident, $gen:expr) => {
        #[test]
        fn $name() {
            let label = stringify!($name);
            $crate::golden::test_yaml_txt($prefix, &label, $gen);
        }
    };
}

/// Create a test which calls [`test_toml_txt`].
///
/// # Example
///
/// ```ignore
///        golden_toml! { "genesis", genesis, Genesis::arbitrary}
/// ```
#[macro_export]
macro_rules! golden_toml {
    ($prefix:literal, $name:ident, $gen:expr) => {
        #[test]
        fn $name() {
            let label = stringify!($name);
            $crate::golden::test_toml_txt($prefix, &label, $gen);
        }
    };
}

/// Create a test which calls [`test_cid`].
///
/// # Example
///
/// ```ignore
///    golden_cid! { "fvm", message, |g| SignedMessage::arbitrary(g).message, |m| SignedMessage::cid(m).unwrap() }
/// ```
#[macro_export]
macro_rules! golden_cid {
    ($prefix:literal, $name:ident, $gen:expr, $cid:expr) => {
        #[test]
        fn $name() {
            let label = stringify!($name);
            let data = $crate::golden::test_cbor_txt($prefix, &label, $gen);
            $crate::golden::test_cid($prefix, &label, data, $cid);
        }
    };
}
