// Copyright 2022-2024 Ikechukwu Ahiara Marvellous (@literallymarvellous)
// SPDX-License-Identifier: MIT
//
// Forked from https://github.com/literallymarvellous/merkle-tree-rs with assumed MIT license
// as per Cargo.toml: https://github.com/literallymarvellous/merkle-tree-rs/blob/d4abd1ca716e65d05e577e2f22b69947bef5b843/Cargo.toml#L5
//
// License headers added post-fork.
use anyhow::{anyhow, bail, Result};
use ethers::utils::{hex, keccak256};
use std::{collections::HashMap, result::Result::Ok};

pub type Hash = ethers::types::H256;

#[derive(PartialEq, Debug)]
pub struct MultiProof<T, U> {
    pub(crate) leaves: Vec<T>,
    pub(crate) proof: Vec<U>,
    pub(crate) proof_flags: Vec<bool>,
}

pub fn hash_pair(a: &Hash, b: &Hash) -> Hash {
    let mut s = [a.as_ref(), b.as_ref()];
    s.sort();
    let bytes = s.concat();
    Hash::from(keccak256(bytes))
}

pub fn left_child_index(i: usize) -> usize {
    2 * i + 1
}

pub fn right_child_index(i: usize) -> usize {
    2 * i + 2
}

pub fn parent_index(i: usize) -> Result<usize> {
    if i > 0 {
        Ok((i - 1) / 2)
    } else {
        Err(anyhow!("Root has no parent"))
    }
}

pub fn sibling_index(i: i32) -> Result<usize> {
    if i > 0 {
        let r = i - (-1i32).pow((i % 2).try_into().unwrap());
        Ok(r as usize)
    } else {
        Err(anyhow!("Root has no sibling"))
    }
}

pub fn is_tree_node(tree: &[Hash], i: usize) -> bool {
    i < tree.len()
}

pub fn is_internal_node(tree: &[Hash], i: usize) -> bool {
    is_tree_node(tree, left_child_index(i))
}

pub fn is_leaf_node(tree: &[Hash], i: usize) -> bool {
    is_tree_node(tree, i) && !is_internal_node(tree, i)
}

pub fn check_tree_node(tree: &[Hash], i: usize) -> Result<()> {
    if is_tree_node(tree, i) {
        Ok(())
    } else {
        Err(anyhow!("Index is not in tree"))
    }
}

pub fn check_internal_node(tree: &[Hash], i: usize) -> Result<()> {
    if is_internal_node(tree, i) {
        Ok(())
    } else {
        Err(anyhow!("Index is not in tree"))
    }
}

pub fn check_leaf_node(tree: &[Hash], i: usize) -> Result<()> {
    if !is_leaf_node(tree, i) {
        bail!("Index is not in tree");
    }
    Ok(())
}

pub fn make_merkle_tree(leaves: Vec<Hash>) -> Result<Vec<Hash>> {
    if leaves.is_empty() {
        bail!("Expected non-zero number of leaves")
    };

    let tree_length = 2 * leaves.len() - 1;
    let mut tree: Vec<Hash> = vec![Hash::zero(); tree_length];

    leaves
        .iter()
        .enumerate()
        .for_each(|(i, v)| tree[tree_length - 1 - i] = v.clone());

    for i in (0..tree_length - leaves.len()).rev() {
        let left_child = &tree[left_child_index(i)];
        let right_child = &tree[right_child_index(i)];
        tree[i] = hash_pair(left_child, right_child);
    }

    Ok(tree)
}

pub fn get_proof(tree: Vec<Hash>, mut i: usize) -> Result<Vec<Hash>> {
    check_leaf_node(&tree, i)?;

    let mut proof = Vec::new();

    while i > 0 {
        let sibling_i = sibling_index(i.try_into()?)?;
        proof.push(tree[sibling_i].clone());
        i = parent_index(i)?;
    }

    Ok(proof)
}

pub fn process_proof(leaf: &Hash, proof: &[Hash]) -> Result<Hash> {
    Ok(proof.iter().fold(leaf.clone(), |a, b| hash_pair(&a, b)))
}

pub fn get_multi_proof(tree: Vec<Hash>, indices: &mut [usize]) -> Result<MultiProof<Hash, Hash>> {
    for i in indices.iter() {
        check_leaf_node(&tree, *i)?;
    }
    indices.sort_by(|a, b| b.cmp(a));

    if indices
        .iter()
        .skip(1)
        .enumerate()
        .any(|(i, v)| *v == indices[i])
    {
        bail!("Cannot prove duplicated index")
    }

    let mut stack = indices[..].to_vec();
    let mut proof: Vec<Hash> = Vec::new();
    let mut proof_flags: Vec<bool> = Vec::new();

    while !stack.is_empty() && stack[0] > 0 {
        let j = stack.remove(0);
        let s = sibling_index(j.try_into()?)?;
        let p = parent_index(j)?;

        if !stack.is_empty() && s == stack[0] {
            proof_flags.push(true);
            stack.remove(0);
        } else {
            proof_flags.push(false);
            proof.push(tree[s].clone());
        }

        stack.push(p);
    }

    if indices.is_empty() {
        proof.push(tree[0].clone());
    }

    Ok(MultiProof {
        leaves: indices.iter().map(|i| tree[*i].clone()).collect(),
        proof,
        proof_flags,
    })
}

pub fn process_multi_proof(multi_proof: &MultiProof<Hash, Hash>) -> Result<Hash> {
    if multi_proof.proof.len() < multi_proof.proof_flags.iter().filter(|&&b| !b).count() {
        bail!("Invalid multiproof format")
    }

    if multi_proof.leaves.len() + multi_proof.proof.len() != multi_proof.proof_flags.len() + 1 {
        bail!("Provide leaves and multi_proof are not compatible")
    }

    let mut stack = multi_proof.leaves[..].to_vec();
    let mut proof = multi_proof.proof[..].to_vec();

    for flag in &multi_proof.proof_flags {
        let a = stack.remove(0);
        let b = if *flag {
            stack.remove(0)
        } else {
            proof.remove(0)
        };
        stack.push(hash_pair(&a, &b))
    }

    if let Some(b) = stack.pop() {
        return Ok(b);
    }
    Ok(proof.remove(0))
}

pub fn is_valid_merkle_tree(tree: Vec<Hash>) -> bool {
    for (i, node) in tree.iter().enumerate() {
        let l = left_child_index(i);
        let r = right_child_index(i);

        if r >= tree.len() {
            if l < tree.len() {
                return false;
            }
        } else if !node.eq(&hash_pair(&tree[l], &tree[r])) {
            return false;
        }
    }

    !tree.is_empty()
}

pub fn render_merkle_tree(tree: &[Hash]) -> Result<String> {
    if tree.is_empty() {
        bail!("Expected non-zero number of nodes");
    }

    let mut stack = vec![0];
    let mut lines: Vec<String> = Vec::new();
    let mut parent_graph = HashMap::new();
    let mut path: Vec<Vec<usize>> = Vec::new();

    while !stack.is_empty() {
        let index = stack.pop().unwrap();
        let current_path = path.pop().unwrap_or_default();

        match current_path.len() {
            0 => {
                let s1 = index.to_string()
                    + ") "
                    + &format!("0x{}", hex::encode(tree.get(index).unwrap()));
                lines.push(s1);
            }
            _ => {
                let s1 = &current_path[..current_path.len() - 1]
                    .iter()
                    .map(|p| vec!["   ", "│  "][*p])
                    .collect::<Vec<&str>>()
                    .join("");
                let s2 = &current_path[current_path.len() - 1..]
                    .iter()
                    .map(|p| vec!["└─ ", "├─ "][*p])
                    .collect::<Vec<&str>>()
                    .join("");
                let s3 = index.to_string()
                    + ") "
                    + &format!("0x{}", hex::encode(tree.get(index).unwrap()));

                lines.push(s1.to_owned() + s2 + &s3);
            }
        }

        if right_child_index(index) < tree.len() {
            parent_graph.insert(index, [left_child_index(index), right_child_index(index)]);
            stack.push(right_child_index(index));
            path.push([current_path.clone(), vec![0]].concat());
            stack.push(left_child_index(index));
            path.push([current_path, vec![1]].concat());
        }
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree() -> Vec<Hash> {
        let tree = vec![
            Hash::from([
                115, 209, 118, 200, 5, 4, 69, 77, 194, 99, 240, 121, 27, 47, 159, 212, 239, 185,
                42, 0, 241, 72, 77, 142, 45, 32, 88, 158, 8, 61, 44, 11,
            ]),
            Hash::from([
                206, 8, 250, 120, 108, 113, 57, 176, 105, 92, 78, 166, 155, 96, 168, 176, 157, 57,
                37, 199, 165, 0, 152, 41, 72, 109, 244, 215, 70, 159, 202, 146,
            ]),
            Hash::from([
                230, 18, 175, 174, 238, 192, 61, 110, 232, 8, 30, 90, 33, 224, 209, 91, 37, 85,
                171, 114, 56, 219, 231, 210, 62, 217, 230, 42, 18, 28, 139, 203,
            ]),
            Hash::from([
                233, 80, 165, 147, 77, 183, 162, 199, 17, 207, 58, 7, 225, 101, 161, 93, 18, 143,
                70, 211, 166, 76, 208, 229, 24, 100, 67, 52, 237, 111, 198, 96,
            ]),
            Hash::from([
                15, 164, 23, 177, 133, 189, 185, 36, 130, 179, 11, 37, 19, 14, 240, 222, 25, 13,
                39, 28, 169, 28, 138, 102, 28, 45, 64, 166, 30, 143, 108, 92,
            ]),
            Hash::from([
                233, 88, 165, 147, 77, 183, 162, 199, 170, 207, 58, 67, 225, 101, 161, 93, 18, 143,
                7, 211, 166, 76, 248, 229, 224, 113, 67, 52, 237, 131, 198, 96,
            ]),
            Hash::from([
                157, 164, 23, 177, 133, 189, 185, 36, 130, 79, 11, 7, 190, 14, 240, 222, 55, 123,
                39, 238, 169, 228, 138, 102, 8, 45, 64, 166, 3, 143, 48, 92,
            ]),
        ];

        tree
    }

    #[test]
    fn test_hash_pair() {
        let a = Hash::from([
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32,
        ]);
        let b = Hash::from([
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31,
        ]);

        let h = hash_pair(&a, &b);
        let e = keccak256([
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
        ]);

        assert_eq!(h.as_bytes(), &e);
    }

    #[test]
    fn test_make_merkle_tree() {
        let byte = Hash::from([
            157, 164, 23, 177, 133, 189, 185, 36, 130, 79, 11, 7, 190, 14, 240, 222, 55, 123, 39,
            238, 169, 228, 138, 102, 8, 45, 64, 166, 3, 143, 48, 92,
        ]);
        let byte2 = Hash::from([
            233, 88, 165, 147, 77, 183, 162, 199, 170, 207, 58, 67, 225, 101, 161, 93, 18, 143, 7,
            211, 166, 76, 248, 229, 224, 113, 67, 52, 237, 131, 198, 96,
        ]);
        let byte3 = Hash::from([
            15, 164, 23, 177, 133, 189, 185, 36, 130, 179, 11, 37, 19, 14, 240, 222, 25, 13, 39,
            28, 169, 28, 138, 102, 28, 45, 64, 166, 30, 143, 108, 92,
        ]);
        let byte4 = Hash::from([
            233, 80, 165, 147, 77, 183, 162, 199, 17, 207, 58, 7, 225, 101, 161, 93, 18, 143, 70,
            211, 166, 76, 208, 229, 24, 100, 67, 52, 237, 111, 198, 96,
        ]);

        let leaves = vec![byte, byte2, byte3, byte4];

        let tree = make_merkle_tree(leaves).unwrap();

        let expected_tree = make_tree();

        assert_eq!(tree, expected_tree);
    }

    #[test]
    fn test_get_proof() {
        let expected_tree = make_tree();

        let proof = get_proof(expected_tree, 6).unwrap();
        let expected_proof = vec![
            Hash::from([
                233, 88, 165, 147, 77, 183, 162, 199, 170, 207, 58, 67, 225, 101, 161, 93, 18, 143,
                7, 211, 166, 76, 248, 229, 224, 113, 67, 52, 237, 131, 198, 96,
            ]),
            Hash::from([
                206, 8, 250, 120, 108, 113, 57, 176, 105, 92, 78, 166, 155, 96, 168, 176, 157, 57,
                37, 199, 165, 0, 152, 41, 72, 109, 244, 215, 70, 159, 202, 146,
            ]),
        ];

        assert_eq!(proof, expected_proof);
    }

    #[test]
    fn test_process_proof() {
        let leaf = Hash::from([
            157, 164, 23, 177, 133, 189, 185, 36, 130, 79, 11, 7, 190, 14, 240, 222, 55, 123, 39,
            238, 169, 228, 138, 102, 8, 45, 64, 166, 3, 143, 48, 92,
        ]);
        let proof = vec![
            Hash::from([
                233, 88, 165, 147, 77, 183, 162, 199, 170, 207, 58, 67, 225, 101, 161, 93, 18, 143,
                7, 211, 166, 76, 248, 229, 224, 113, 67, 52, 237, 131, 198, 96,
            ]),
            Hash::from([
                206, 8, 250, 120, 108, 113, 57, 176, 105, 92, 78, 166, 155, 96, 168, 176, 157, 57,
                37, 199, 165, 0, 152, 41, 72, 109, 244, 215, 70, 159, 202, 146,
            ]),
        ];

        let expected_root = Hash::from([
            115, 209, 118, 200, 5, 4, 69, 77, 194, 99, 240, 121, 27, 47, 159, 212, 239, 185, 42, 0,
            241, 72, 77, 142, 45, 32, 88, 158, 8, 61, 44, 11,
        ]);

        let root = process_proof(&leaf, &proof).unwrap();
        assert_eq!(root, expected_root)
    }

    #[test]
    fn test_get_multi_proof() {
        let tree = make_tree();

        let multi_proof = get_multi_proof(tree, &mut [4, 6]).unwrap();
        let expected_multi_proof = MultiProof {
            leaves: [
                Hash::from([
                    157, 164, 23, 177, 133, 189, 185, 36, 130, 79, 11, 7, 190, 14, 240, 222, 55,
                    123, 39, 238, 169, 228, 138, 102, 8, 45, 64, 166, 3, 143, 48, 92,
                ]),
                Hash::from([
                    15, 164, 23, 177, 133, 189, 185, 36, 130, 179, 11, 37, 19, 14, 240, 222, 25,
                    13, 39, 28, 169, 28, 138, 102, 28, 45, 64, 166, 30, 143, 108, 92,
                ]),
            ]
            .to_vec(),
            proof: [
                Hash::from([
                    233, 88, 165, 147, 77, 183, 162, 199, 170, 207, 58, 67, 225, 101, 161, 93, 18,
                    143, 7, 211, 166, 76, 248, 229, 224, 113, 67, 52, 237, 131, 198, 96,
                ]),
                Hash::from([
                    233, 80, 165, 147, 77, 183, 162, 199, 17, 207, 58, 7, 225, 101, 161, 93, 18,
                    143, 70, 211, 166, 76, 208, 229, 24, 100, 67, 52, 237, 111, 198, 96,
                ]),
            ]
            .to_vec(),
            proof_flags: [false, false, true].into(),
        };

        assert_eq!(multi_proof, expected_multi_proof);
    }

    #[test]
    fn test_process_multi_proof() {
        let multi_proof = MultiProof {
            leaves: [
                Hash::from([
                    157, 164, 23, 177, 133, 189, 185, 36, 130, 79, 11, 7, 190, 14, 240, 222, 55,
                    123, 39, 238, 169, 228, 138, 102, 8, 45, 64, 166, 3, 143, 48, 92,
                ]),
                Hash::from([
                    15, 164, 23, 177, 133, 189, 185, 36, 130, 179, 11, 37, 19, 14, 240, 222, 25,
                    13, 39, 28, 169, 28, 138, 102, 28, 45, 64, 166, 30, 143, 108, 92,
                ]),
            ]
            .to_vec(),
            proof: [
                Hash::from([
                    233, 88, 165, 147, 77, 183, 162, 199, 170, 207, 58, 67, 225, 101, 161, 93, 18,
                    143, 7, 211, 166, 76, 248, 229, 224, 113, 67, 52, 237, 131, 198, 96,
                ]),
                Hash::from([
                    233, 80, 165, 147, 77, 183, 162, 199, 17, 207, 58, 7, 225, 101, 161, 93, 18,
                    143, 70, 211, 166, 76, 208, 229, 24, 100, 67, 52, 237, 111, 198, 96,
                ]),
            ]
            .to_vec(),
            proof_flags: [false, false, true].into(),
        };
        let root = process_multi_proof(&multi_proof).unwrap();
        let expected_root = Hash::from([
            115, 209, 118, 200, 5, 4, 69, 77, 194, 99, 240, 121, 27, 47, 159, 212, 239, 185, 42, 0,
            241, 72, 77, 142, 45, 32, 88, 158, 8, 61, 44, 11,
        ]);
        assert_eq!(root, expected_root);
    }

    #[test]
    fn test_is_valid_merkle_tree() {
        let tree = make_tree();

        assert!(is_valid_merkle_tree(tree));
    }

    #[test]
    fn test_render_merkle_tree() {
        let tree = make_tree();

        let render = render_merkle_tree(&tree).unwrap();
        println!("tree: \n {}", render);
    }
}
