// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

/// JSON based test so we can parse data from the disk where it's nice to be human readable.
mod json {
    use fendermint_testing::golden_json;
    use fendermint_vm_genesis::Genesis;
    use quickcheck::Arbitrary;
    golden_json! { "genesis/json", genesis, Genesis::arbitrary }
}

/// CBOR based tests to make sure we can parse data in network format.
mod cbor {
    use fendermint_testing::golden_cbor;
    use fendermint_vm_genesis::Genesis;
    use quickcheck::Arbitrary;
    golden_cbor! { "genesis/cbor", genesis, Genesis::arbitrary }
}
