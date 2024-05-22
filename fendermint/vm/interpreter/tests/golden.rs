// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

/// JSON based test in case we want to configure the FvmStateParams by hand.
mod json {
    use fendermint_testing::golden_json;
    use fendermint_vm_interpreter::fvm::state::FvmStateParams;
    use quickcheck::Arbitrary;
    golden_json! { "fvmstateparams/json", fvmstateparams, FvmStateParams::arbitrary }
}

/// CBOR based tests in case we have to grab FvmStateParams from on-chain storage.
mod cbor {
    use fendermint_testing::golden_cbor;
    use fendermint_vm_interpreter::fvm::state::FvmStateParams;
    use quickcheck::Arbitrary;
    golden_cbor! { "fvmstateparams/cbor", fvmstateparams, FvmStateParams::arbitrary }
}
