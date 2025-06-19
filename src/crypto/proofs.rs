use serde::{Deserialize, Serialize};

use crate::{crypto::{hashing::HashFunction, merkle::MerkleTree}, nodes::node::StdByteArray};



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

/// Generate a Merkle proof for a specific transaction
pub fn generate_proof_of_inclusion(merkle_tree: &MerkleTree, data: StdByteArray, hash_function: &mut impl HashFunction) -> Option<MerkleProof> {
    let leaves = merkle_tree.leaves.as_ref()?;
    let nodes = &merkle_tree.nodes;
    let root_key = merkle_tree.root?;

    // Hash the data
    hash_function.update(data);
    let target_hash = hash_function.digest().expect("Hashing failed");

    // Find matching leaf
    let mut current_key = *leaves.iter().find(|&&key| nodes[key].hash == target_hash)?;
    
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MerkleTrieProof {
    pub steps: Vec<ProofStep>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProofStep {
    pub index_taken: u8, // 0-15: which nibble was followed
    pub siblings: Vec<(u8, StdByteArray)>, // all other occupied child hashes at this level
}