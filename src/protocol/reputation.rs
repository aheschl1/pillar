
use crate::{blockchain::chain_shard::ChainShard, crypto::hashing::HashFunction, reputation::NodeHistory};



pub fn compute_mining_reputation(
    shard: &ChainShard, 
    miner: [u8; 32],
    hash_function: &mut impl HashFunction
){
    // we will start by etracting the history of the node
    let history = NodeHistory::extract(shard, miner, hash_function);
    // now we have the blocks mined by the miner
    // reputation will be built on the number of blocks mined and the number of transactions in the blocks
    // furthermore, timestamp is taken into account
    // a brand new block is worth 1 reputation - it reduces exponentially over time
    let mut reputation: f32;
    
}