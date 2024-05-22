// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

/// Take the first transactions until the first one that would exceed the maximum limit.
///
/// The function does not skip or reorder transaction even if a later one would stay within the limit.
pub fn take_until_max_size<T: AsRef<[u8]>>(txs: Vec<T>, max_tx_bytes: usize) -> Vec<T> {
    let mut size: usize = 0;
    let mut out = Vec::new();
    for tx in txs {
        let bz: &[u8] = tx.as_ref();
        if size.saturating_add(bz.len()) > max_tx_bytes {
            break;
        }
        size += bz.len();
        out.push(tx);
    }
    out
}
