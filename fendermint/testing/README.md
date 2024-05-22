# Testing

The `fendermint_testing` crate (ie. the current directory) provides some reusable utilities that can be imported into _other_ tests. These are behind feature flags:

* `golden`: helper functions for writing tests with golden files
* `arb`: provides `quickcheck::Arbitrary` instances for some things which are problematic in the FVM library, such as `Address` and `TokenAmount`.
* `smt`: small framework for State Machine Testing (a.k.a. Model Testing)


# End to end tests

Beyond this, for no other reason than code organisation, the directory has sub-projects, which contain actual tests.

For example the [smoke-test](./smoke-test/) is a a crate that uses `cargo make` to start a local stack with Tendermint and Fendermint running in Docker, and run some integration tests, which can be found in the [Makefile.toml](./smoke-test/Makefile.toml).

To run these, either `cd` into that directory and run them from there, or run all from the root using `make e2e`, which also builds the docker images.
