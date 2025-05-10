
use core::f64;
use std::cmp::min;

use crate::{blockchain::chain_shard::ChainShard, crypto::hashing::HashFunction, reputation::NodeHistory};


const MINING_WORTH_HALF_LIFE: f64 = 8f64;
const MINING_WORTH_MAX: f64 = 1f64;

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
    let mut reputation: f64 = 0.0;
    for block in history.blocks_mined.iter() {
        reputation += block_worth_scaling_fn(block.timestamp);
    }
    
}

/// this function will scale the worth of a block over time
/// the worth of a block is 1 at the time of mining
/// Based on a half life centered around the current time
/// 0.5^((current_time - block_time) / MINING_WORTH_HALF_LIFE)
/// Beware of floating point errors
/// 
/// # Arguments
/// * `block_time` - the time of mining in seconds since epoch
fn block_worth_scaling_fn(block_time: u64) -> f64 {
    // days since epoch
    let current_time = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as f64) / 60.0 / 60.0 / 24.0;
    
    let block_time = block_time as f64 / 60.0 / 60.0 / 24.0;

    let diff = block_time - current_time;
    let worth = (0.5f64).powf(-diff / MINING_WORTH_HALF_LIFE);
    f64::min(worth, MINING_WORTH_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_worth_scaling_fn_now() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let worth = block_worth_scaling_fn(block_time);
        assert_eq!(worth, 1.0);
    }

    #[test]
    fn test_block_worth_scaling_fn_halflife_days_ago() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 - (MINING_WORTH_HALF_LIFE * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64);
        assert_eq!(worth, MINING_WORTH_MAX / 2.0);
    }

    #[test]
    fn test_block_worth_scaling_fn_halflife_days_ahead() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 + (MINING_WORTH_HALF_LIFE * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64);
        assert_eq!(worth, MINING_WORTH_MAX);
    }

    #[test]
    fn test_zero_convergence() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 - (1000.0 * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64);
        assert!(worth < 1e-10);
    }

    #[test]
    fn test_two_half_life() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 - (2.0*MINING_WORTH_HALF_LIFE * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64);
        assert_eq!(worth, MINING_WORTH_MAX / 4.0);
    }
}