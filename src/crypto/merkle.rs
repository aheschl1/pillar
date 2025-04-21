use crate::blockchain::transaction::Transaction;

use super::hashing::{HashFunction, Hashable, Sha3_256Hash};

pub struct TreeNode{
    // left is the left child of the node
    pub left: Option<Box<TreeNode>>,
    // right is the right child of the node
    pub right: Option<Box<TreeNode>>,
    // hash is the sha3_256 hash of the node
    pub hash: [u8; 32],
}

// copy
impl Clone for TreeNode {
    fn clone(&self) -> Self {
        TreeNode {
            left: self.left.clone(),
            right: self.right.clone(),
            hash: self.hash,
        }
    }
}

pub trait ZKProof<T: Hashable, F: HashFunction> {
    /// Generate a Merkle tree from the given data
    fn generate_tree(&self, data: Vec<&T>, hash_function: &mut F) -> TreeNode;
    /// Generate a proof of inclusion for the given data in the Merkle tree
    fn generate_proof_of_inclusion(&self, data: &T, tree: &TreeNode, hash_function: &mut F) -> Vec<[u8; 32]>;
    /// Verify the proof of inclusion for the given data in the Merkle tree
    fn verify_proof_of_inclusion(&self, data: &T, proof: Vec<[u8; 32]>, root: [u8; 32], hash_function: &mut F) -> bool;
}

pub struct MerkleTree{
    // root is the root of the Merkle tree
    pub root: TreeNode,
}

// zkproof impl for merkle
impl ZKProof<Transaction, Sha3_256Hash> for MerkleTree {
    /// Generate a Merkle tree from the given data
    /// 
    /// # Arguments
    /// 
    /// * `data` - A vector of references to Transaction objects
    /// * `hash_function` - A mutable instance of a type implementing the HashFunction trait
    /// 
    /// TODO: This is inneficient, as it clones the data twice 
    fn generate_tree(&self, data: Vec<&Transaction>, hash_function: &mut Sha3_256Hash) -> TreeNode {
        // list of transactions to list of leaves
        let mut data: Vec<TreeNode> = data.clone().into_iter().map(|transaction|{
            hash_function.update(transaction.hash);
            TreeNode { left: None, right: None, hash: hash_function.digest().expect("Hashing failed") }
        }).collect();

        // work up levels, adding references to the previous level
        // until we reach the root
        while data.len() > 1 {
            if data.len() % 2 != 0 {
                data.push(data.last().unwrap().clone());
            }
            data = data.chunks(2)
                .map(|nodes|{
                    // nodes is a slice of two TreeNode
                    hash_function.update(nodes[0].hash);
                    hash_function.update(nodes[1].hash);
                    TreeNode { 
                        left: Some(Box::new(nodes[0].clone())), 
                        right: Some(Box::new(nodes[1].clone())), 
                        hash: hash_function.digest().expect("Hashing failed")
                    }
                }).collect();
        }
        data[0].clone()
    }

    fn generate_proof_of_inclusion(&self, data: &Transaction, tree: &TreeNode, hash_function: &mut Sha3_256Hash) -> Vec<[u8; 32]> {
        todo!()
    }

    fn verify_proof_of_inclusion(&self, data: &Transaction, proof: Vec<[u8; 32]>, root: [u8; 32], hash_function: &mut Sha3_256Hash) -> bool {
        todo!()
    }
}
