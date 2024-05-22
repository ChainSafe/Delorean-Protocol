# IPC Developer Guide

## Troubleshooting Cargo build issues

This project has a large set of dependencies and they are all bundled together in a root Cargo.lock file. This means that sometimes, when upgrading some of our dependencies, Cargo will do something unexpected which causes build errors which can be very time consuming to figure out.

### Failed to select a version for `xyz`.

<details>
<summary>Example</summary>

```
error: failed to select a version for `fvm_sdk`.
    ... required by package `frc42_dispatch v5.0.0`
    ... which satisfies dependency `frc42_dispatch = "^5.0.0"` of package `fil_actor_account v12.0.0 (/home/fridrik/workspace4/builtin-actors/actors/account)`
    ... which satisfies path dependency `fil_actor_account` (locked to 12.0.0) of package `fil_actor_miner v12.0.0 (/home/fridrik/workspace4/builtin-actors/actors/miner)`
    ... which satisfies path dependency `fil_actor_miner` (locked to 12.0.0) of package `fil_actors_integration_tests v1.0.0 (/home/fridrik/workspace4/builtin-actors/integration_tests)`
    ... which satisfies path dependency `fil_actors_integration_tests` (locked to 1.0.0) of package `test_vm v12.0.0 (/home/fridrik/workspace4/builtin-actors/test_vm)`
versions that meet the requirements `~4.0` are: 4.0.0

all possible versions conflict with previously selected packages.

  previously selected package `fvm_sdk v4.1.1`
    ... which satisfies dependency `fvm_sdk = "^4.1.0"` (locked to 4.1.1) of package `fil_actors_runtime v12.0.0 (/home/fridrik/workspace4/builtin-actors/runtime)`
    ... which satisfies path dependency `fil_actors_runtime` (locked to 12.0.0) of package `fil_actor_account v12.0.0 (/home/fridrik/workspace4/builtin-actors/actors/account)`
    ... which satisfies path dependency `fil_actor_account` (locked to 12.0.0) of package `fil_actor_miner v12.0.0 (/home/fridrik/workspace4/builtin-actors/actors/miner)`
    ... which satisfies path dependency `fil_actor_miner` (locked to 12.0.0) of package `fil_actors_integration_tests v1.0.0 (/home/fridrik/workspace4/builtin-actors/integration_tests)`
    ... which satisfies path dependency `fil_actors_integration_tests` (locked to 1.0.0) of package `test_vm v12.0.0 (/home/fridrik/workspace4/builtin-actors/test_vm)`
```
</details>

If you get this error, then it means that Rust could not find a version of the `xyz` crate which fulfills the requirements of the package and other packages that depend on it. To debug this, look what dependencies of `xyz` package are, and check if they need to be updated.

This error can happen for example when upgrading to a new major/minor FVM versions without upgrading also other dependencies like `frc_dispatch` which requires fvm as well. In that case we must upgrade the `frc_dispatch` package to use the same FVM version as we are using.


### Unexplained transitive dependencies in wasm32 target after upgrading FVM version

When upgrading FVM dependency (from 4.0 to 4.1) it resulted in our `fendermint/actors/build.rs` script to fail due Cargo including multiple new dependencies in the `wasm32` target which did not occur before and caused build errors since these new dependencies did not support Wasm target.

By running `cargo tree` we saw that these dependencies were pulled in from the `filecoin-proofs-api` required by `fvm_shared`. This dependency is pulled in when requiring `fvm_shared` with the `crypto` feature. Looking at our different Cargo.toml files, we noticed that `contracts/binding/Cargo.toml` file was the only one setting that feature. We needed to remove the `crypto` feature, compile, and then add it back in for the wasm build (and tests) to succeed.

## Troubleshooting Misc Cargo related issues

### Unexplained behaviour due to local changes in ~/.cargo/registry

<details>
<summary>Example: Failing integration tests due to local changes in  ~/.cargo/registry</summary>

```
Taken from: https://filecoinproject.slack.com/archives/C04JR5R1UL8/p1706886901218709

Ok, here todays learning for me... I have been debugging for a good while today why my local smoke-test integration test was failing.
I thought this was related to me upgrading the different cargo make scripts for the stuff I am doing to enable tracing but couldn't figure it out. I thought this was also due to me upgrading my docker installation as I get the error also on main branch, so I tested this on my mac instead of local linux workstation and it worked there, hmm what could be going on?

So I had tried cargo clean, removing all docker images/containers but nothing worked. So I used the debugger and found out I HAD ACCIDENTALLY changed one line in the  .cargo/registry/src/index.crates.io-6f17d22bba15001f/ethers-providers-2.0.11/src/rpc/provider.rs , I don't know how or when that happened, its a mystery but my guess accidental paste in my editor, but who knows..

If something like this happen again, I will make sure to try to remove .cargo/registry altogether to make sure there are no local changes.
If someone knows if its possible to check if there are any local changes in the .cargo/registry I would be interested to know, but as far as I know there is no way to do that
```
</details>

If you are seeing weird unexplained behaviour that you kind of can't wrap your head around, then you may want to delete your `~/.cargo/registry` and run `cargo build`. Here is why, you _might_ have accidentally changed some of the crates's source files that cargo is using in your project. There is no way to know if you had made any local changes to any of these crates as `Cargo`` does not maintain hash of these dependencies and there is no git repo available to compare against.
