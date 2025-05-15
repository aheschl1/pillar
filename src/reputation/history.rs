use std::{cmp::max, collections::HashSet};

use crate::{blockchain::{chain_shard::ChainShard, TrimmableChain}, crypto::hashing::{HashFunction, Hashable}, primitives::block::{BlockHeader, BlockTail}, protocol::reputation::{block_worth_scaling_fn, BLOCK_STAMP_SCALING}};

/// The reputation structure holds all the information needed to compute the reputation of a node
/// This information should be stored by each node, and each node can add it to a side chain
#[derive(Debug, Default)]
pub struct NodeHistory{
    /// The public key of the node
    pub public_key: [u8; 32],
    /// The blocks that have been mined by the node - could be empty if the node does not mine
    pub blocks_mined: Vec<BlockHeader>,
    /// the blocks that have been stamped by the node - could be empty if the node does not stamp
    pub blocks_stamped: Vec<BlockHeader>,
    /// the max chain depth at the time of computation
    pub max_chain_depth: u64,
}

impl NodeHistory{
    pub fn new(
        public_key: [u8; 32],
        blocks_mined: Vec<BlockHeader>,
        blocks_stamped: Vec<BlockHeader>,
        max_chain_depth: u64,
    ) -> Self {
        NodeHistory {
            public_key,
            max_chain_depth,
            blocks_stamped,
            blocks_mined
        }
    }
    /// Returns the reputation of the node
    pub fn extract(
        shard: &mut ChainShard,
        miner: [u8; 32],
        hash_function: &mut impl HashFunction
    ) -> NodeHistory{
        // trim the shard so that we elminate old forks that are now diregarded
        shard.trim();
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
            blocks_stamped: vec![], // TODO: add this
            max_chain_depth,
        }
    }

    /// Settle a new block into the history of the node
    /// This is used when the node is a miner and has mined a new block
    pub fn settle_head(&mut self, block: BlockHeader){
        if block.miner_address.is_none() || block.miner_address.unwrap() != self.public_key{
            panic!("Block does not belong to this miner");
        }
        self.blocks_mined.push(block);
        // update the max chain depth
        self.max_chain_depth = max(self.max_chain_depth, block.depth);
    }

    /// Settle a new block into the history of the node
    /// This is used when the node has stamped a new block
    pub fn settle_tail(&mut self, tail: &BlockTail, head: BlockHeader){
        let stampers = tail.get_stampers();
        if !stampers.contains(&self.public_key){
            panic!("Block does not belong to this peer");
        }
        // now push the block into the history
        self.blocks_stamped.push(head);
        // update the max chain depth
        self.max_chain_depth = max(self.max_chain_depth, head.depth);
    }

    pub fn compute_mining_reputation(
        &self
    ) -> f64{
        // we have the blocks mined by the miner
        // reputation will be built on the number of blocks mined and the number of transactions in the blocks
        // furthermore, timestamp is taken into account
        // a brand new block is worth 1 reputation - it reduces exponentially over time
        let mut reputation: f64 = 0.0;
        for block in self.blocks_mined.iter() {
            reputation += block_worth_scaling_fn(block.timestamp);
        }
        reputation
    }

    pub fn compute_block_stamp_reputation(
        &self
    ) -> f64{
        // we have the blocks mined by the miner
        // reputation will be built on the number of blocks mined and the number of transactions in the blocks
        // furthermore, timestamp is taken into account
        // a brand new block is worth 1 reputation - it reduces exponentially over time
        let mut reputation: f64 = 0.0;
        for block in self.blocks_stamped.iter() {
            reputation += block_worth_scaling_fn(block.timestamp)*BLOCK_STAMP_SCALING;
        }
        reputation
    }

    /// Compute the reputation of the node
    /// This is the sum of the mining reputation and the stamping reputation
    pub fn compute_reputation(
        &self
    ) -> f64{
        let mining_reputation = self.compute_mining_reputation();
        let stamping_reputation = self.compute_block_stamp_reputation();
        // the reputation is the sum of the two
        mining_reputation + stamping_reputation
    }
}