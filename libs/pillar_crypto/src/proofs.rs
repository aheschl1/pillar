use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::{hashing::{HashFunction, Hashable}, merkle::MerkleTree, merkle_trie::{to_nibbles, MerkleTrie}, types::StdByteArray};



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

// ============================================================================================
// Trie proofs to follow
// TODO generalize

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrieMerkleProof {
    pub steps: Vec<ProofStep>,
}

impl TrieMerkleProof {
    pub fn new(steps: Vec<ProofStep>) -> Self {
        TrieMerkleProof { steps }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct ProofStep {
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
                hash_function.update([self.native]);
                hash_function.update(native_data);
            }
            // update the hash with the sibling hash
            hash_function.update([*index]);
            hash_function.update(hash);
        }
        // maybe native comes last
        if last_index.is_none() || last_index.unwrap() < self.native {
            hash_function.update([self.native]);
            hash_function.update(native_data);
        }
        if let Some((i, value)) = &self.value {
            hash_function.update([*i]);
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
            hash_function.update([*index]);
            hash_function.update(hash);
        }
        hash_function.update([self.value.as_ref().unwrap().0]);
        hash_function.update(native_data);
        hash_function.digest().expect("Hashing failed")
    }      
}

impl TrieMerkleProof {
    pub fn verify(&self, native_data: Vec<u8>, root_hash: StdByteArray, hash_function: &mut impl HashFunction) -> bool {
        let steps = self.steps.iter().rev().collect::<Vec<_>>();
        let mut current_hash = steps[0].compute_level_first(native_data, hash_function);
        for step in &steps[1..] {
            current_hash = step.compute_level(current_hash, hash_function);
        }
        current_hash == root_hash
    }
}

/// Generate a Merkle proof for a specific value in the Merkle Trie.
/// # Arguments
/// * `target_key` - The key for which the proof is generated.
pub fn generate_proof_of_state<K, V>(
    merkle_trie: &MerkleTrie<K, V>, 
    target_key: K, 
    root: Option<StdByteArray>, 
    hash_function: &mut impl HashFunction
) -> Option<(TrieMerkleProof, V)> 
where K: Hashable, V: Serialize + for<'a> Deserialize<'a>
{
    let value = merkle_trie.get(&target_key, root.expect("Root must be provided"));
    value.as_ref()?;
    let path = to_nibbles(&target_key);
    let mut steps: Vec<ProofStep> = Vec::new();
    
    let mut current_node_key = Some(*merkle_trie.roots.get(&root.expect("Root must be provided")).unwrap());
    // work the way down, computing the proof steps along the way.
    let mut nibble = Some(path[0]);
    let mut i = 0;
    while let Some(key) = current_node_key {
        let current_node = merkle_trie.nodes.get(key).expect("Node not found");
        let mut siblings: Vec<(u8, StdByteArray)> = Vec::new();
        for (j, child) in current_node.children.iter().enumerate() {
            if (nibble.is_none() || j != nibble.unwrap() as usize) && child.is_some() {
                let sibling_key = child.unwrap();
                let sibling_hash = merkle_trie.get_hash_for(sibling_key, hash_function).expect("Sibling hash not found");
                siblings.push((j as u8, sibling_hash));
            }
        }
        steps.push(ProofStep {
            native: nibble.unwrap_or(0), // where is the nibble in the path
            siblings,
            value: current_node.value.clone().map(|value| (current_node.children.len() as u8, value))
        });
        current_node_key = match nibble {
            Some(n) => {
                i += 1;
                nibble = if i < path.len() { Some(path[i]) } else { None };
                current_node.children[n as usize]
            },
            None => None
        };
    }
        
    Some((TrieMerkleProof::new(steps), value.unwrap()))
}