use core::hash;
use std::{collections::HashMap, hash::Hash};

use serde::{Deserialize, Serialize};

use crate::{crypto::{hashing::{DefaultHash, HashFunction, Hashable}, merkle::MerkleTree}, nodes::node::StdByteArray};



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
pub struct MerkleProofV2 {
    pub steps: Vec<ProofStep>,
}

impl MerkleProofV2 {
    pub fn new(steps: Vec<ProofStep>) -> Self {
        MerkleProofV2 { steps }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProofStep {
    // native is the index of the value on which they are constructing the proof
    pub native: u8,
    // Give the indices and hashes of the siblings in the trie
    // MUST BE SORTED BY THE INDEX
    pub siblings: Vec<(u8, StdByteArray)>,
    // value
    pub value: Option<(u8, Vec<u8>)>,
}

impl ProofStep {
    fn compute_level(&self, native_data: StdByteArray, hash_function: &mut impl HashFunction) -> StdByteArray {
        // here, we reconstruct the hash of the level
        let mut last_index: Option<u8> = None;
        for (i, (index, hash)) in self.siblings.iter().enumerate() {
            // sanity check
            if last_index.is_some() && last_index.unwrap() >= *index{
                panic!("Invalid proof step: indices must be strictly sorted");
            }
            last_index = Some(*index);
            // actual work
            // check if we include the native data in this level
            if *index > self.native && (i == 0 || self.siblings[i-1].0 < self.native) {
                hash_function.update(&[self.native]);
                hash_function.update(native_data);
            }
            // update the hash with the sibling hash
            hash_function.update(&[*index]);
            hash_function.update(hash);
        }
        // maybe native comes last
        if last_index.is_none() || last_index.unwrap() < self.native {
            hash_function.update(&[self.native]);
            hash_function.update(native_data);
        }
        if let Some((i, value)) = &self.value {
            hash_function.update(&[*i]);
            hash_function.update(value);
        }
        hash_function.digest().expect("Hashing failed")
    }

    fn compute_level_first(&self, native_data: impl AsRef<[u8]>, hash_function: &mut impl HashFunction) -> StdByteArray {
        // here, we reconstruct the hash of the level
        let mut last_index: Option<u8> = None;
        for (index, hash) in self.siblings.iter() {
            // sanity check
            if last_index.is_some() && last_index.unwrap() >= *index{
                panic!("Invalid proof step: indices must be strictly sorted");
            }
            last_index = Some(*index);
            // actual work
            // update the hash with the sibling hash
            hash_function.update(&[*index]);
            hash_function.update(hash);
        }
        hash_function.update(&[self.value.as_ref().unwrap().0]);
        hash_function.update(native_data);
        hash_function.digest().expect("Hashing failed")
    }      
}

impl MerkleProofV2 {
    pub fn verify(&self, native_data: Vec<u8>, root_hash: StdByteArray, hash_function: &mut impl HashFunction) -> bool {
        let steps = self.steps.iter().rev().collect::<Vec<_>>();
        let mut current_hash = steps[0].compute_level_first(native_data, hash_function);
        for step in &steps[1..] {
            current_hash = step.compute_level(current_hash, hash_function);
        }
        current_hash == root_hash
    }
}

pub trait Provable<K> where K: Hashable {
    /// Generate a Merkle proof for a specific value in the structure.
    fn generate_proof_for_value(&self, target_value: K, root: Option<StdByteArray>, hash_function: &mut impl HashFunction) -> Option<MerkleProofV2>;
    /// generate a proof for a specific key in the structure.
    fn generate_proof_for_key(&self, target_key: K, root: Option<StdByteArray>, hash_function: &mut impl HashFunction) -> Option<MerkleProofV2>;
}

impl<K: Hashable> Provable<K> for MerkleTree {
    fn generate_proof_for_value(&self, target_value: K, root: Option<StdByteArray>, hash_function: &mut impl HashFunction) -> Option<MerkleProofV2> {
        todo!()
    }
    
    fn generate_proof_for_key(&self, target_key: K, root: Option<StdByteArray>, hash_function: &mut impl HashFunction) -> Option<MerkleProofV2> {
        unimplemented!("generate_proof_for_key is not implemented yet");
    }
}