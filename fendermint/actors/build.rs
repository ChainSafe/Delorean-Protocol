// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_bundler::Bundler;
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

const ACTORS: &[&str] = &["chainmetadata", "eam"];

const FILES_TO_WATCH: &[&str] = &["Cargo.toml", "src"];

fn main() -> Result<(), Box<dyn Error>> {
    // Cargo executable location.
    let cargo = std::env::var_os("CARGO").expect("no CARGO env var");

    let out_dir = std::env::var_os("OUT_DIR")
        .as_ref()
        .map(Path::new)
        .map(|p| p.join("bundle"))
        .expect("no OUT_DIR env var");
    println!("cargo:warning=out_dir: {:?}", &out_dir);

    let manifest_path =
        Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR unset"))
            .join("Cargo.toml");

    for file in [FILES_TO_WATCH, ACTORS].concat() {
        println!("cargo:rerun-if-changed={}", file);
    }

    // Cargo build command for all test_actors at once.
    let mut cmd = Command::new(cargo);
    cmd.arg("build")
        .args(
            ACTORS
                .iter()
                .map(|pkg| "-p=fendermint_actor_".to_owned() + pkg),
        )
        .arg("--target=wasm32-unknown-unknown")
        .arg("--profile=wasm")
        .arg("--features=fil-actor")
        .arg(format!("--manifest-path={}", manifest_path.display()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // We are supposed to only generate artifacts under OUT_DIR,
        // so set OUT_DIR as the target directory for this build.
        .env("CARGO_TARGET_DIR", &out_dir)
        // As we are being called inside a build-script, this env variable is set. However, we set
        // our own `RUSTFLAGS` and thus, we need to remove this. Otherwise cargo favors this
        // env variable.
        .env_remove("CARGO_ENCODED_RUSTFLAGS");

    // Print out the command line we're about to run.
    println!("cargo:warning=cmd={:?}", &cmd);

    // Launch the command.
    let mut child = cmd.spawn().expect("failed to launch cargo build");

    // Pipe the output as cargo warnings. Unfortunately this is the only way to
    // get cargo build to print the output.
    let stdout = child.stdout.take().expect("no stdout");
    let stderr = child.stderr.take().expect("no stderr");
    let j1 = thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            println!("cargo:warning={:?}", line.unwrap());
        }
    });
    let j2 = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            println!("cargo:warning={:?}", line.unwrap());
        }
    });

    j1.join().unwrap();
    j2.join().unwrap();

    let result = child.wait().expect("failed to wait for build to finish");
    if !result.success() {
        return Err("actor build failed".into());
    }

    // make sure the output dir exists
    std::fs::create_dir_all("output")
        .expect("failed to create output dir for the custom_actors_bundle.car file");

    let dst = Path::new("output/custom_actors_bundle.car");
    let mut bundler = Bundler::new(dst);
    for (&pkg, id) in ACTORS.iter().zip(1u32..) {
        let bytecode_path = Path::new(&out_dir)
            .join("wasm32-unknown-unknown/wasm")
            .join(format!("fendermint_actor_{}.wasm", pkg));

        // This actor version doesn't force synthetic CIDs; it uses genuine
        // content-addressed CIDs.
        let forced_cid = None;

        let cid = bundler
            .add_from_file(id, pkg.to_owned(), forced_cid, &bytecode_path)
            .unwrap_or_else(|err| {
                panic!(
                    "failed to add file {:?} to bundle for actor {}: {}",
                    bytecode_path, id, err
                )
            });
        println!(
            "cargo:warning=added {} ({}) to bundle with CID {}",
            pkg, id, cid
        );
    }
    bundler.finish().expect("failed to finish bundle");

    println!("cargo:warning=bundle={}", dst.display());

    Ok(())
}
