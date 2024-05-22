// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

mod json {
    use fendermint_materializer::manifest::Manifest;
    use fendermint_testing::golden_json;
    use quickcheck::Arbitrary;
    golden_json! { "manifest/json", manifest, Manifest::arbitrary }
}

mod yaml {
    use fendermint_materializer::manifest::Manifest;
    use fendermint_testing::golden_yaml;
    use quickcheck::Arbitrary;
    golden_yaml! { "manifest/yaml", manifest, Manifest::arbitrary }
}

mod toml {
    use fendermint_materializer::manifest::Manifest;
    use fendermint_testing::golden_toml;
    use quickcheck::Arbitrary;
    golden_toml! { "manifest/toml", manifest, Manifest::arbitrary }
}
