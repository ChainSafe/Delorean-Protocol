// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

/// JSON based test so we can parse data from the disk where it's nice to be human readable.
mod json {
    use fendermint_testing::golden_json;
    use fendermint_vm_snapshot::SnapshotManifest;
    use quickcheck::Arbitrary;
    golden_json! { "manifest/json", manifest, SnapshotManifest::arbitrary }
}

/// CBOR based test to make sure we can parse data in network format and we also cover the state params.
mod cbor {
    use fendermint_testing::golden_cbor;
    use fendermint_vm_snapshot::SnapshotManifest;
    use quickcheck::Arbitrary;
    golden_cbor! { "manifest/cbor", manifest, SnapshotManifest::arbitrary }
}
