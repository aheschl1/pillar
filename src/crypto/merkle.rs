use std::hash::Hash;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};


use crate::nodes::node::StdByteArray;

use super::hashing::{HashFunction, Hashable};

// Define a key type for our nodes
new_key_type! {
    pub struct NodeKey;
}


#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TreeNode {
    pub left: Option<NodeKey>,
    pub right: Option<NodeKey>,
    pub parent: Option<NodeKey>,
    pub hash: StdByteArray,
}

#[derive(Debug, Clone, Deserialize, Serialize, Hash, PartialEq, Eq)]
pub enum HashDirection {
    Left,
    Right,
}

#[derive(Debug, Clone, Deserialize, Serialize, Hash, PartialEq, Eq)]
pub struct MerkleProof {
    pub hashes: Vec<StdByteArray>,
    pub directions: Vec<HashDirection>,
    pub root: StdByteArray,
}

#[derive(Debug, Clone)]
pub struct MerkleTree {
    pub nodes: SlotMap<NodeKey, TreeNode>,
    pub root: Option<NodeKey>,
    pub leaves: Option<Vec<NodeKey>>,
}

impl Hash for MerkleTree{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for (key, node) in self.nodes.iter() {
            node.hash(state);
            key.hash(state);
        }
        self.root.hash(state);
        self.leaves.hash(state);
    }
}

impl PartialEq for MerkleTree {
    fn eq(&self, other: &Self) -> bool {
        // compare hash
        self.get_root_hash() == other.get_root_hash()
    }
}

impl Eq for MerkleTree {
    
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

impl MerkleTree {
    pub fn new() -> Self {
        MerkleTree {
            nodes: SlotMap::with_key(),
            root: None,
            leaves: None,
        }
    }

    pub fn get_root_hash(&self) -> Option<StdByteArray>{
        match self.root{
            None => None,
            Some(root) => {
                let root = self.nodes.get(root);
                if let Some(root) = root {
                    Some(root.hash)
                }else{
                    None
                }
            }
        }
    }
}

/// Generate a Merkle tree from the given data
pub fn generate_tree(data: Vec<&impl Hashable>, hash_function: &mut impl HashFunction) -> Result<MerkleTree, std::io::Error> {
    if data.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Data is empty"));
    }

    let mut tree = MerkleTree::new();

    // Create leaves
    let mut leaves: Vec<NodeKey> = data.into_iter().map(|item| {
        let item_hash = item.hash(hash_function).expect("hashing failed");
        hash_function.update(item_hash);
        let node = TreeNode {
            left: None,
            right: None,
            parent: None,
            hash: hash_function.digest().expect("Hashing failed"),
        };
        tree.nodes.insert(node)
    }).collect();
    
    let leaves_clone = leaves.clone();

    // Build up the tree
    while leaves.len() > 1 {
        if leaves.len() % 2 != 0 {
            leaves.push(*leaves.last().unwrap());
        }

        leaves = leaves.chunks(2).map(|pair| {
            let (left_key, right_key) = (pair[0], pair[1]);
            let left_hash = tree.nodes[left_key].hash;
            let right_hash = tree.nodes[right_key].hash;

            hash_function.update(left_hash);
            hash_function.update(right_hash);

            let new_node = TreeNode {
                left: Some(left_key),
                right: Some(right_key),
                parent: None,
                hash: hash_function.digest().expect("Hashing failed"),
            };

            let parent_key = tree.nodes.insert(new_node);
            
            tree.nodes[left_key].parent = Some(parent_key);
            tree.nodes[right_key].parent = Some(parent_key);

            parent_key
        }).collect();
    }

    tree.root = Some(leaves[0]);
    tree.leaves = Some(leaves_clone);

    Ok(tree)
}

/// Generate a Merkle proof for a specific transaction
pub fn generate_proof_of_inclusion(merkle_tree: &MerkleTree, data: StdByteArray, hash_function: &mut impl HashFunction) -> Option<MerkleProof> {
    let leaves = merkle_tree.leaves.as_ref()?;
    let nodes = &merkle_tree.nodes;
    let root_key = merkle_tree.root?;

    // Hash the data
    hash_function.update(data);
    let target_hash = hash_function.digest().expect("Hashing failed");

    // Find matching leaf
    let mut current_key = leaves.iter().find(|&&key| nodes[key].hash == target_hash)?.clone();
    
    let mut hashes = Vec::new();
    let mut directions = Vec::new();

    while let Some(parent_key) = nodes[current_key].parent {
        let parent = &nodes[parent_key];
        if parent.left == Some(current_key) {
            if let Some(right_key) = parent.right {
                hashes.push(nodes[right_key].hash);
                directions.push(HashDirection::Right);
            }
        } else if parent.right == Some(current_key) {
            if let Some(left_key) = parent.left {
                hashes.push(nodes[left_key].hash);
                directions.push(HashDirection::Left);
            }
        }
        current_key = parent_key;
    }

    Some(MerkleProof {
        hashes,
        directions,
        root: nodes[root_key].hash,
    })
}

/// Verify a Merkle proof
pub fn verify_proof_of_inclusion<T: Into<StdByteArray>>(data: T, proof: &MerkleProof, root: StdByteArray, hash_function: &mut impl HashFunction) -> bool {
    hash_function.update(data.into());
    let mut current_hash = hash_function.digest().expect("Hashing failed");

    for (hash, direction) in proof.hashes.iter().zip(proof.directions.iter()) {
        match direction {
            HashDirection::Left => {
                hash_function.update(hash);
                hash_function.update(current_hash);
            }
            HashDirection::Right => {
                hash_function.update(current_hash);
                hash_function.update(hash);
            }
        }
        current_hash = hash_function.digest().expect("Hashing failed");
    }

    current_hash == root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::transaction::Transaction;
    use crate::crypto::hashing::DefaultHash;

    #[test]
    fn test_generate_tree() {
        let mut hash_function = DefaultHash::new();
        let transaction1 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0,
            &mut hash_function
        );
        let transaction2 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1,
            &mut hash_function
        );
        let transaction3 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2,
            &mut hash_function
        );
        let transaction4 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            3,
            &mut hash_function
        );

        let data = vec![&transaction1, &transaction2, &transaction3, &transaction4];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        assert!(merkle_tree.root.is_some());
        assert!(merkle_tree.leaves.is_some());

        let mut height = 0;
        let mut current_node = merkle_tree.root;
        while let Some(node) = current_node {
            let node = merkle_tree.nodes.get(node).unwrap();
            if node.left.is_some() {
                height += 1;
                current_node = node.left;
            } else {
                break;
            }
        }
        assert_eq!(height, 2);
        // verify each parent has two children or 0 children
        let mut current_node = merkle_tree.root;
        while let Some(node) = current_node {
            let node = merkle_tree.nodes.get(node).unwrap();
            if node.left.is_some() {
                assert!(node.right.is_some());
                current_node = node.left;
            } else {
                break;
            }
        }

    }

    #[test]
    fn test_generate_proof_of_inclusion() {
        let mut hash_function = DefaultHash::new();
        let transaction1 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0,
            &mut hash_function
        );
        let transaction2 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1,
            &mut hash_function
        );
        let transaction3 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2,
            &mut hash_function
        );
        let transaction4 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            3,
            &mut hash_function
        );

        let data = vec![&transaction1, &transaction2, &transaction3, &transaction4];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        let proof = generate_proof_of_inclusion(&merkle_tree, transaction1.hash, &mut hash_function).unwrap();
        assert_eq!(proof.hashes.len(), 2);
    }

    #[test]
    fn test_verification_of_proof(){
        let mut hash_function = DefaultHash::new();
        let transaction1 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0,
            &mut hash_function
        );
        let transaction2 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1,
            &mut hash_function
        );
        let transaction3 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2,
            &mut hash_function
        );
        let transaction4 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            3,
            &mut hash_function
        );

        let data = vec![&transaction1, &transaction2, &transaction3, &transaction4];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        let proof = generate_proof_of_inclusion(&merkle_tree, transaction3.hash, &mut hash_function).unwrap();
        assert_eq!(proof.hashes.len(), 2);
        // pass
        assert!(verify_proof_of_inclusion(transaction3, &proof, merkle_tree.nodes[merkle_tree.root.unwrap()].hash, &mut hash_function));
        // fail
        assert!(!verify_proof_of_inclusion(transaction1, &proof, merkle_tree.nodes[merkle_tree.root.unwrap()].hash, &mut hash_function));
    }
    #[test]
    fn test_empty_tree() {
        let mut hash_function = DefaultHash::new();
        let data: Vec<&Transaction> = vec![];
        let merkle_tree = generate_tree(data, &mut hash_function);
        assert!(merkle_tree.is_err());
    }

    #[test]
    fn test_odd_tree() {
        let mut hash_function = DefaultHash::new();
        let transaction1 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0,
            &mut hash_function
        );
        let transaction2 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1,
            &mut hash_function
        );
        let transaction3 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2,
            &mut hash_function
        );

        let data = vec![&transaction1, &transaction2, &transaction3];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        assert!(merkle_tree.root.is_some());
        assert!(merkle_tree.leaves.is_some());
    }

    #[test]
    fn test_odd_tree_proof(){
        let mut hash_function = DefaultHash::new();
        let transaction1 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            0,
            &mut hash_function
        );
        let transaction2 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            1,
            &mut hash_function
        );
        let transaction3 = Transaction::new(
            [0; 32],
            [0; 32],
            0,
            0,
            2,
            &mut hash_function
        );

        let data = vec![&transaction1, &transaction2, &transaction3];
        let merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        let proof = generate_proof_of_inclusion(&merkle_tree, transaction1.hash, &mut hash_function).unwrap();
        assert_eq!(proof.hashes.len(), 2);
        // test verification
        assert!(verify_proof_of_inclusion(transaction1, &proof, merkle_tree.nodes[merkle_tree.root.unwrap()].hash, &mut hash_function));
        // test failure
        assert!(!verify_proof_of_inclusion(transaction2, &proof, merkle_tree.nodes[merkle_tree.root.unwrap()].hash, &mut hash_function));
    }
}
