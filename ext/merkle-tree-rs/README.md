> Forked from [literallymarvellous/merkle-tree-rs](https://github.com/literallymarvellous/merkle-tree-rs) (via [consensus-shipyard/merkle-tree-rs](https://github.com/consensus-shipyard/merkle-tree-rs)) with adaptations, including: type flexibility, perf improvements, ethers dependency upgrade, output formatting, and type safety (using the specialized H256 for hashes, and preserving Bytes for raw bytes). The original upstream was labelled as MIT licensed in its Cargo.toml, so we preserve that license for this crate.

**A Rust library to generate merkle trees and merkle proofs.**

This is based on [@openzeppelin/merkle-tree](https://github.com/OpenZeppelin/merkle-tree) implementation of merkle-trees and it's well suited for airdrops and similar mechanisms in combination with OpenZeppelin Contracts [`MerkleProof`] utilities.

[`merkleproof`]: https://docs.openzeppelin.com/contracts/4.x/api/utils#MerkleProof

## Quick Start

Add merkle-tree-rs to your repository, also serde and serde_json for json.

```
[dependencies]

merkle-tree-rs = "0.1.0"
serde = "1.0.147"
serde_json = "1.0"
```

### Building a Tree

```rust
use merkle_tree_rs::standard::StandardMerkleTree;
use std::fs;

fn main() {
    let values = vec![
        vec![
            "0x1111111111111111111111111111111111111111",
            "5000000000000000000",
        ],
        vec![
            "0x2222222222222222222222222222222222222222",
            "2500000000000000000",
        ],
    ];

    let tree = StandardMerkleTree::of(values, &["address", "uint256"]);

    let root = tree.root();

    println!("Merkle root: {}", root);

    let tree_json = serde_json::to_string(&tree.dump()).unwrap();

    fs::write("tree.json", tree_json).unwrap();
}
```

1. Get the values to include in the tree. (Note: Consider reading them from a file.)
2. Build the merkle tree. Set the encoding to match the values.
3. Print the merkle root. You will probably publish this value on chain in a smart contract.
4. Write a file that describes the tree. You will distribute this to users so they can generate proofs for values in the tree.

### Obtaining a Proof

Assume we're looking to generate a proof for the entry that corresponds to address `0x11...11`.

```rust
    use merkle_tree_rs::standard::StandardMerkleTree;
    use std::fs;

    fn main() {
        let tree_json = fs::read_to_string("tree.json").unwrap();

        let tree_data: StandardMerkleTreeData = serde_json::from_str(&tree_json).unwrap();

        let tree = StandardMerkleTree::load(tree_data).unwrap();

        for (i, v) in tree.clone().enumerate() {
        if v[0] == "0x1111111111111111111111111111111111111111" {
            let proof = tree.get_proof(LeafType::Number(i));
            println!("Value : {:?}", v);
            println!("Proof : {:?}", proof);
        }
        }
    }
```

1. Load the tree from the description that was generated previously.
2. Loop through the entries to find the one you're interested in.
3. Generate the proof using the index of the entry.

In practice this might be done in a frontend application prior to submitting the proof on-chain, with the address looked up being that of the connected wallet.

See [`MerkleProof`] for documentation on how to validate the proof in Solidity.

## Standard Merkle Trees

This library works on "standard" merkle trees designed for Ethereum smart contracts. We have defined them with a few characteristics that make them secure and good for on-chain verification.

- The tree is shaped as a [complete binary tree](https://xlinux.nist.gov/dads/HTML/completeBinaryTree.html).
- The leaves are sorted.
- The leaves are the result of ABI encoding a series of values.
- The hash used is Keccak256.
- The leaves are double-hashed to prevent [second preimage attacks].

[second preimage attacks]: https://flawed.net.nz/2018/02/21/attacking-merkle-trees-with-a-second-preimage-attack/

From the last three points we get that the hash of a leaf in the tree with value `[addr, amount]` can be computed in Solidity as follows:

```solidity
bytes32 leaf = keccak256(bytes.concat(keccak256(abi.encode(addr, amount))));
```

This is an opinionated design that we believe will offer the best out of the box experience for most users. We may introduce options for customization in the future based on user requests.

## API & Examples

### `StandardMerkleTree`

```rust
use merkle_tree_rs::standard::StandardMerkleTree,
```

### `StandardMerkleTree.of`

Types currently supported for encoding includes address, uint, uint256 and string.

```rust
let values = vec![
        vec![
            "0x1111111111111111111111111111111111111111",
            "5000000000000000000",
        ],
        vec![
            "0x2222222222222222222222222222222222222222",
            "2500000000000000000",
        ],
    ];
    let encoding = ["address", "uint256"];
    let tree = StandardMerkleTree::of(values, &encoding).unwrap();
```

Creates a standard merkle tree out of an array of the elements in the tree, along with their types for ABI encoding.

> **Note**
> Consider reading the array of elements from a CSV file for easy interoperability with spreadsheets or other data processing pipelines.

### `tree.root`

```rust
println!("{}", tree.root());
```

The root of the tree is a commitment on the values of the tree. It can be published (e.g., in a smart contract) to later prove that its values are part of the tree.

### `tree.dump`

```rust
let tree_json = serde_json::to_string(&tree.dump()).unwrap();

fs::write("tree.json", tree_json).unwrap();
```

Returns a description of the merkle tree for distribution. It contains all the necessary information to reproduce the tree, find the relevant leaves, and generate proofs. You should distribute this to users in a web application or command line interface so they can generate proofs for their leaves of interest.

### `StandardMerkleTree.load`

```rust
let tree_json = fs::read_to_string("tree.json").unwrap();
let tree_data: StandardMerkleTreeData = serde_json::from_str(&tree_json).unwrap();

let tree = StandardMerkleTree::load(tree_data).unwrap();
```

Loads the tree from a description previously returned by `dump`.

### `tree.getProof`

```rust
let proof = tree.get_proof(LeafType::Number(i)).unwrap();
```

Returns a proof for the `i`th value in the tree. Indices refer to the position of the values in the array from which the tree was constructed.

It is wrapped in a `LeafType` enum of `Number(usize)` for indices and `LeafBytes(Vec<string>)` for values. Using value is less efficient cause it will fail if the value is not found in the tree.

```rust
let proof = tree.get_proof(LeafType::LeafBytes([alice, "100"])).unwrap();
```

### `tree.getMultiProof`

```rust
let multi_proof = tree.get_multi_proof([LeafType::Number(i0), LeafType::Number(i1), ...]).unwrap();
```

Returns a multiproof strcut containing {proof, prooflags, leaves} for the values at indices `i0, i1, ...`. Indices refer to the position of the values in the array from which the tree was constructed.

The multiproof returned contains an array with the leaves that are being proven. This array may be in a different order than that given by `i0, i1, ...`! The order returned is significant, as it is that in which the leaves must be submitted for verification (e.g., in a smart contract).

Also accepts values instead of indices, but this will be less efficient. It will fail if any of the values is not found in the tree.

### Interating over the tree

```rust
for (i, v) in tree.clone().enumerate {
  console.log("value: {:?}", v);
  console.log("proof: {:?}", tree.getProof(LeafType::Number(i)).unwrap());
}
```

Lists the values in the tree along with their indices, which can be used to obtain proofs.

### `tree.render`

```rust
println!("{:?}", tree.render().unwrap());
```

Returns a visual representation of the tree that can be useful for debugging.

### `tree.leafHash`

```rust
let leaf = tree.leaf_hash(["alice".to_string(), "100".to_string()]).unwrap();
```

Returns the leaf hash of the value, as defined in [Standard Merkle Trees](#standard-merkle-trees).

Corresponds to the following expression in Solidity:

```solidity
bytes32 leaf = keccak256(bytes.concat(keccak256(abi.encode(alice, 100))));
```

Attributions

- [@openzeppelin/merkle-tree](https://github.com/OpenZeppelin/merkle-tree)
