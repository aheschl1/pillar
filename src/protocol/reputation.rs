use crate::blockchain::chain_shard::ChainShard;


/// Given a chain shard (chain of headers), compute the reputation of a particular miner
/// Reputation will be based on a combination of the number of blocks mined, and the difficulty of the blocks mined.
/// Reputation 
/// 
/// # Arguments
/// 
/// * `shard` - The chain shard to compute the reputation from
/// * `miner` - The miner to compute the reputation for - public key
pub fn compute_reputation(
    shard: &ChainShard,
    miner: [u8; 32]
) -> u32{

    0
}

pub fn get_reputation_score_from_difficulty(){

}