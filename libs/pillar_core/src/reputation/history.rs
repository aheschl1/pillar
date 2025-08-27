use std::{cmp::max, collections::HashSet};

use pillar_crypto::{hashing::{HashFunction, Hashable}, types::StdByteArray};
use serde::{Deserialize, Serialize};

use crate::{blockchain::{chain_shard::ChainShard, TrimmableChain}, primitives::block::BlockHeader, protocol::reputation::{block_worth_scaling_fn, BLOCK_STAMP_SCALING, N_TRANSMISSION_SIGNATURES}};


#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
struct HeaderShard{
    previous_hash: StdByteArray,
    n_stamps: u32,
    depth: u64,
    timestamp: u64, // timestamp of the block
}

impl From<BlockHeader> for HeaderShard {
    fn from(header: BlockHeader) -> Self {
        HeaderShard {
            previous_hash: header.previous_hash,
            depth: header.depth,
            n_stamps: header.tail.get_stampers().len() as u32, // number of stampers
            timestamp: header.timestamp,
        }
    }
}

/// The reputation structure holds all the information needed to compute the reputation of a node
/// This information should be stored by each node, and each node can add it to a side chain
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct NodeHistory{
    /// The public key of the node
    pub public_key: StdByteArray,
    /// The blocks that have been mined by the node - could be empty if the node does not mine
    blocks_mined: Vec<HeaderShard>,
    /// the blocks that have been stamped by the node - could be empty if the node does not stamp
    blocks_stamped: Vec<HeaderShard>,
}

impl NodeHistory{
    pub fn new(
        public_key: StdByteArray
    ) -> Self {
        NodeHistory {
            public_key,
            blocks_mined: vec![],
            blocks_stamped: vec![]
        }
    }
    /// Returns the reputation of the node
    pub fn extract(
        shard: &mut ChainShard,
        miner: StdByteArray,
        hash_function: &mut impl HashFunction
    ) -> NodeHistory{
        // trim the shard so that we elminate old forks that are now diregarded
        shard.trim();
        // we will start at each leaf, and track the blocks that have been mined by the miner.
        let mut blocks_mined: Vec<HeaderShard> = vec![];
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
                if current_block.completion.expect("Block should be complete").miner_address == miner{
                    blocks_mined.push(current_block.into());
                }
                curr = shard.get_block(&current_block.previous_hash); // recurse
            }
        }

        NodeHistory { 
            public_key: miner,
            blocks_mined,
            blocks_stamped: vec![], // TODO: add this
        }
    }

    /// Settle a new block into the history of the node
    /// This is used when the node is a miner and has mined a new block
    pub fn settle_miner(&mut self, block: BlockHeader){
        self.blocks_mined.push(block.into());
    }

    /// Settle a new block into the history of the node
    /// This is used when the node has stamped a new block
    pub fn settle_stampers(&mut self, head: BlockHeader){
        let stampers = head.tail.get_stampers();
        if !stampers.contains(&self.public_key){
            panic!("Block does not belong to this peer");
        }
        // now push the block into the history
        self.blocks_stamped.push(head.into());
    }

    pub fn compute_mining_reputation(
        &self,
        current_time: u64
    ) -> f64{
        // we have the blocks mined by the miner
        // reputation will be built on the number of blocks mined and the number of transactions in the blocks
        // furthermore, timestamp is taken into account
        // a brand new block is worth 1 reputation - it reduces exponentially over time
        let mut reputation: f64 = 0.0;
        for shard in self.blocks_mined.iter() {
            let n: f64 = N_TRANSMISSION_SIGNATURES as f64;
            let stamp_boost_fn = (shard.n_stamps as f64 + n) / n; // extra reward for mining when there are stamps
            reputation += block_worth_scaling_fn(shard.timestamp, current_time) * stamp_boost_fn;
        }
        reputation
    }

    pub fn compute_block_stamp_reputation(
        &self,
        current_time: u64
    ) -> f64{
        // we have the blocks mined by the miner
        // reputation will be built on the number of blocks mined and the number of transactions in the blocks
        // furthermore, timestamp is taken into account
        // a brand new block is worth 1 reputation - it reduces exponentially over time
        let mut reputation: f64 = 0.0;
        for block in self.blocks_stamped.iter() {
            reputation += block_worth_scaling_fn(block.timestamp, current_time)*BLOCK_STAMP_SCALING;
        }
        reputation
    }

    /// Compute the reputation of the node
    /// This is the sum of the mining reputation and the stamping reputation
    pub fn compute_reputation(
        &self,
        current_time: u64
    ) -> f64{
        let mining_reputation = self.compute_mining_reputation(current_time);
        let stamping_reputation = self.compute_block_stamp_reputation(current_time);
        // the reputation is the sum of the two
        mining_reputation + stamping_reputation
    }

    pub fn n_blocks_mined(&self) -> usize {
        self.blocks_mined.len()
    }

    pub fn n_blocks_stamped(&self) -> usize {
        self.blocks_stamped.len()
    }
}