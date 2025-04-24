use std::{hash::Hash, iter::Zip, sync::{Arc, Mutex}};

use serde::{Serialize, Deserialize};

use crate::primitives::transaction::Transaction;

use super::hashing::{HashFunction, DefaultHash};

#[derive(Debug)]
pub struct TreeNode{
    // left is the left child of the node
    pub left: Option<Arc<Mutex<TreeNode>>>,
    // right is the right child of the node
    pub right: Option<Arc<Mutex<TreeNode>>>,
    // parent is the parent of the node
    pub parent: Option<Arc<Mutex<TreeNode>>>,
    // hash is the sha3_256 hash of the node
    pub hash: [u8; 32],
}

// copy
impl Clone for TreeNode {
    fn clone(&self) -> Self {
        TreeNode {
            left: self.left.clone(),
            right: self.right.clone(),
            parent: self.parent.clone(),
            hash: self.hash.clone(),
        }
    }
}

pub enum HashDirection {
    Left,
    Right,
}

pub struct MerkleProof{
    // hash is the hash of the node
    pub hashes: Vec<[u8; 32]>,
    // direction is the direction of the node
    pub directions: Vec<HashDirection>,
    // root is the root of the tree
    pub root: [u8; 32]
}

/// Merkle tree struct
#[derive(Debug, Clone)]
pub struct MerkleTree{
    // root is the root of the Merkle tree
    pub root: Option<Arc<Mutex<TreeNode>>>,
    // store the leaves for logn proof generation
    pub leaves: Option<Vec<Arc<Mutex<TreeNode>>>>,
}

impl Default for MerkleTree {
    fn default() -> Self {
        MerkleTree::new()
    }
}

impl MerkleTree {
    pub fn new() -> Self {
        MerkleTree {
            root: None,
            leaves: None,
        }
    }
}

/// Generate a Merkle tree from the given data
/// 
/// # Arguments
/// 
/// * `data` - A vector of references to Transaction objects
/// * `hash_function` - A mutable instance of a type implementing the HashFunction trait
/// 
/// TODO: This is inneficient, as it clones the data twice 
pub fn generate_tree(data: Vec<&Transaction>, hash_function: &mut impl HashFunction) -> Result<MerkleTree, std::io::Error> {
    if data.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Data is empty"));
    }
    // list of transactions to list of leaves
    let mut data: Vec<Arc<Mutex<TreeNode>>> = data.into_iter().map(|transaction|{
        hash_function.update(transaction.hash);
        let node = TreeNode { left: None, right: None, parent:None, hash: hash_function.digest().expect("Hashing failed") };
        // add to leaves
        Arc::new(Mutex::new(node))
    }).collect();
    let leaves = Some(data.clone());

    // work up levels, adding references to the previous level
    // until we reach the root
    while data.len() > 1 {
        if data.len() % 2 != 0 {
            data.push(data.last().unwrap().clone());
        }
        data = data.chunks(2)
            .map(|nodes|{
                // nodes is a slice of two TreeNode
                hash_function.update(nodes[0].lock().unwrap().hash);
                hash_function.update(nodes[1].lock().unwrap().hash);
                // new node
                let new_node = TreeNode { 
                    left: Some(nodes[0].clone()), 
                    right: Some(nodes[1].clone()), 
                    parent: None,
                    hash: hash_function.digest().expect("Hashing failed")
                };
                let new_node = Arc::new(Mutex::new(new_node));
                // parent pointer
                if let Ok(mut left_lock) = nodes[0].lock() {
                    left_lock.parent = Some(new_node.clone());
                }
                if let Ok(mut right_lock) = nodes[1].lock() {
                    right_lock.parent = Some(new_node.clone());
                }
                
                return new_node;
            }).collect();
    }
    let merkle = MerkleTree {
        root: Some(data[0].clone()),
        leaves: leaves,
    };
    Ok(merkle)
}

/// Generate a Merkle tree from the given data
/// 
/// # Arguments
/// 
/// * `data` - A reference to the Transaction object for which to generate the proof
/// * `hash_function` - A mutable instance of a type implementing the HashFunction trait
/// 
/// # Returns
/// 
/// * `Ok(())` if the tree was generated successfully
/// * `Err(std::io::Error)` if the tree could not be generated
pub fn generate_proof_of_inclusion(merkle_tree: &MerkleTree, data: &Transaction, hash_function:  &mut impl HashFunction) -> Option<MerkleProof> {
    if merkle_tree.leaves.is_none() {
        return None;
    }
    if merkle_tree.root.is_none() {
        return None;
    }
    // hash of hash is leaf
    hash_function.update(data.hash);
    let leaf_hash = hash_function.digest().expect("Hashing failed");
    let leaves = merkle_tree.leaves.as_ref().unwrap();
    // find the leaf
    let current_node = leaves.iter().find(|node| node.lock().unwrap().hash == leaf_hash);
    if current_node.is_none() {
        return None;
    }
    // proof
    // let mut proof = Vec::new();
    let mut directions = Vec::new();
    let mut hashes = Vec::new();

    let mut current_node = current_node.unwrap().clone();

    loop{ 
        let node = current_node.lock().unwrap();
        let hash = node.hash;
        let parent = node.parent.clone();
        drop(node);
        // if not root
        if let Some(parent_node) = parent {
            let parent = parent_node.lock().unwrap();
            if parent.left.as_ref().map(|node| node.lock().unwrap().hash) == Some(hash) {
                // proof.push((parent.right.as_ref().unwrap().lock().unwrap().hash, HashDirection::Right));
                hashes.push(parent.right.as_ref().unwrap().lock().unwrap().hash);
                directions.push(HashDirection::Right);
            } else {
                // proof.push((parent.left.as_ref().unwrap().lock().unwrap().hash, HashDirection::Left));
                hashes.push(parent.left.as_ref().unwrap().lock().unwrap().hash);
                directions.push(HashDirection::Left);
            }
            current_node = parent_node.clone();
        } else {
            break;
        }
    }

    Some(MerkleProof {
        hashes,
        directions,
        root: merkle_tree.root.as_ref().unwrap().lock().unwrap().hash,
    })
    
}

/// Verify the proof of inclusion for a given transaction
/// 
/// # Arguments
/// 
/// * `data` - A reference to the Transaction object for which to verify the proof
/// * `proof` - A vector of tuples containing the hash and direction of each node in the proof
/// * `root` - The root hash of the Merkle tree
/// * `hash_function` - A mutable instance of a type implementing the HashFunction trait
/// 
/// # Returns
/// 
/// * `true` if the proof is valid
/// * `false` if the proof is invalid
pub fn verify_proof_of_inclusion(data: &Transaction, proof: &MerkleProof, root: [u8; 32], hash_function:  &mut impl HashFunction) -> bool {
    hash_function.update(data.hash);
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
        let mut current_node = merkle_tree.root.clone();
        while let Some(node) = current_node {
            let node = node.lock().unwrap();
            if node.left.is_some() {
                height += 1;
                current_node = node.left.clone();
            } else {
                break;
            }
        }
        assert_eq!(height, 2);
        // verify each parent has two children or 0 children
        let mut current_node = merkle_tree.root.clone();
        while let Some(node) = current_node {
            let node = node.lock().unwrap();
            if node.left.is_some() {
                assert!(node.right.is_some());
                current_node = node.left.clone();
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
        let mut merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        let proof = generate_proof_of_inclusion(&mut merkle_tree, &transaction1, &mut hash_function).unwrap();
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
        let mut merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        let proof = generate_proof_of_inclusion(&mut merkle_tree, &transaction3, &mut hash_function).unwrap();
        assert_eq!(proof.hashes.len(), 2);
        // pass
        assert!(verify_proof_of_inclusion(&transaction3, &proof, merkle_tree.root.as_ref().unwrap().lock().unwrap().hash, &mut hash_function));
        // fail
        assert!(!verify_proof_of_inclusion(&transaction1, &proof, merkle_tree.root.as_ref().unwrap().lock().unwrap().hash, &mut hash_function));
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
        let mut merkle_tree = generate_tree(data, &mut hash_function).unwrap();
        let proof = generate_proof_of_inclusion(&mut merkle_tree, &transaction1, &mut hash_function).unwrap();
        assert_eq!(proof.hashes.len(), 2);
        // test verification
        assert!(verify_proof_of_inclusion(&transaction1, &proof, merkle_tree.root.as_ref().unwrap().lock().unwrap().hash, &mut hash_function));
        // test failure
        assert!(!verify_proof_of_inclusion(&transaction2, &proof, merkle_tree.root.as_ref().unwrap().lock().unwrap().hash, &mut hash_function));
    }
}
