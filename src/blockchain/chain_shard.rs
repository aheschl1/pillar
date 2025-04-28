use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::primitives::block::BlockHeader;

use super::chain::Chain;

/// chain shard is used to build up a chain given a list of block headers
/// It is responsible for the validation and construction of the chain from a new node.

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChainShard {
    nodes: HashMap<[u8; 32], BlockHeader>,
}

impl ChainShard {
    pub fn new(chain: &Chain) -> Self {
        let nodes: HashMap<[u8; 32], BlockHeader> = chain.blocks.iter().map(|(hash, block)| (hash.clone(), block.header.clone())).collect();
        // trim to only have deepest fork, and fork depth of 10 less than the max depth
        let depth = chain.depth;
        
    }


    /// Adds new block headers to the chain shard
    /// Does not validate the headers
    pub fn update(&mut self, headers: Vec<BlockHeader>) {
        for header in headers.iter() {
            let hash = header.hash();
            if self.nodes.contains_key(&hash) {
                continue;
            }
            self.nodes.insert(hash, header);
            self.leaves.remove(&header.previous_hash);
            self.leaves.insert(hash);
        }
    }

    /// Validate the chain
    ///   1. Check that all block depths are valid
    ///   2. Check for floating blocks
    ///   3. Check for duplicate blocks
    ///   4. Remove forks of depth 10 less than the max depth
    ///   5. Ensure that all "previous blocks" exist
    ///   6. Ensure the genisis block exists
    fn validate(&self){

    }

}
