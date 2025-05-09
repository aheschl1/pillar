use std::{cmp::max, collections::HashSet};

use crate::{blockchain::chain_shard::ChainShard, crypto::hashing::{HashFunction, Hashable}, nodes::node::Node, primitives::block::BlockHeader};

/// This is a TODO module

/// The reputation structure holds all the information needed to compute the reputation of a node
/// This information should be stored by each node, and each node can add it to a side chain
pub struct NodeHistory{
    /// The public key of the node
    pub public_key: [u8; 32],
    /// The blocks that have been mined by the node - could be empty if the node does not mine
    pub blocks_mined: Vec<BlockHeader>,
    /// the max chain depth at the time of computation
    pub max_chain_depth: u64,
    /// peer distributions - timestamps
    pub peer_distribution: Option<Vec<u64>>,
    /// timestamps of block distributions to new nodes - without errors
    pub block_distributions: Option<Vec<u64>>
}

impl NodeHistory{
    pub fn new(
        public_key: [u8; 32],
        blocks_mined: Vec<BlockHeader>,
        max_chain_depth: u64,
        peer_distribution: Option<Vec<u64>>,
        block_distributions: Option<Vec<u64>>
    ) -> Self {
        NodeHistory {
            public_key,
            max_chain_depth,
            blocks_mined,
            peer_distribution,
            block_distributions
        }
    }
    /// Returns the reputation of the node
    pub fn extract(
        shard: &ChainShard,
        miner: [u8; 32],
        hash_function: &mut impl HashFunction
    ) -> NodeHistory{
        // we will start at each leaf, and track the blocks that have been mined by the miner.
        let mut blocks_mined = vec![];
        let mut blocks_seen = HashSet::new();
        let mut max_chain_depth: u64 = 0;
        for leaf in shard.leaves.iter(){
            let mut curr = shard.get_block(leaf);
            max_chain_depth = max(max_chain_depth, shard.get_block(leaf).unwrap().depth);
            while let Some(current_block) = curr{
                let hash = current_block.hash(hash_function).unwrap();
                if blocks_seen.contains(&hash){
                    // we have seen this block before
                    break;
                }
                blocks_seen.insert(hash);
                if current_block.miner_address.expect("no miner address on header") == miner{
                    blocks_mined.push(current_block);
                }
                curr = shard.get_block(&current_block.previous_hash); // recurse
            }
        }

        NodeHistory { 
            public_key: miner,
            blocks_mined,
            max_chain_depth,
            peer_distribution: None,
            block_distributions: None
        }
    }
}