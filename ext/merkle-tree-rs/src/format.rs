// Copyright 2022-2024 Ikechukwu Ahiara Marvellous (@literallymarvellous)
// SPDX-License-Identifier: MIT
//
// Forked from https://github.com/literallymarvellous/merkle-tree-rs with assumed MIT license
// as per Cargo.toml: https://github.com/literallymarvellous/merkle-tree-rs/blob/d4abd1ca716e65d05e577e2f22b69947bef5b843/Cargo.toml#L5
//
// License headers added post-fork.
use std::borrow::Cow;

use crate::core::Hash;

pub trait FormatHash {
    type Out;

    fn format(hash: Cow<Hash>) -> Self::Out;
}

/// Format hashes as 0x prefixed hexadecimal string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hex0x;

impl FormatHash for Hex0x {
    type Out = String;

    fn format(hash: Cow<Hash>) -> Self::Out {
        format!("0x{}", ethers::utils::hex::encode(hash.as_ref()))
    }
}

/// Return hashes as bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Raw;

impl FormatHash for Raw {
    type Out = Hash;

    fn format(hash: Cow<Hash>) -> Self::Out {
        hash.into_owned()
    }
}
