use super::hashing::{HashFunction, Hashable};

pub struct TreeNode{
    // left is the left child of the node
    pub left: Option<Box<TreeNode>>,
    // right is the right child of the node
    pub right: Option<Box<TreeNode>>,
    // hash is the sha3_256 hash of the node
    pub hash: [u8; 32],
}

pub trait ZKProof<T: Hashable, F: HashFunction> {
    /// Generate a Merkle tree from the given data
    fn generate_tree(&self, data: Vec<&T>, hash_function: &mut F) -> TreeNode;
    /// Generate a proof of inclusion for the given data in the Merkle tree
    fn generate_proof_of_inclusion(&self, data: &T, tree: &TreeNode, hash_function: &mut F) -> Vec<[u8; 32]>;
    /// Verify the proof of inclusion for the given data in the Merkle tree
    fn verify_proof_of_inclusion(&self, data: &T, proof: Vec<[u8; 32]>, root: [u8; 32], hash_function: &mut F) -> bool;
}