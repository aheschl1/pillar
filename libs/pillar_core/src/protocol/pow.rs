use std::cmp::min;

use flume::Receiver;
use pillar_crypto::{hashing::{HashFunction, Hashable}, merkle::MerkleTree, merkle_trie::MerkleTrie, types::StdByteArray};


use crate::{accounting::{account::Account, state::StateManager}, primitives::block::{Block, BlockHeader}};

use super::difficulty::get_base_difficulty_from_depth;

pub const POR_THRESHOLD: f64 = 50f64;
pub const POR_INCLUSION_MINIMUM: f64 = 1f64;

pub fn is_valid_hash(difficulty: u64, hash: &StdByteArray) -> bool {
    // check for 'difficulty' leading 0 bits
    let mut leading_zeros: u64 = 0;
    for byte in hash.iter() {
        if *byte == 0 {
            leading_zeros += 8;
        } else {
            leading_zeros += byte.leading_zeros() as u64;
            break;
        }
    }
    leading_zeros >= difficulty
}

/// Get the difficulty for a block based on its header and the state trie
/// This function enables swap to PoR (Proof of Reputation) mining
/// Difficulty is reduced if the cummulative reputation of the stampers is above a threshold
/// If the cummulative reputation is above the threshold, we reduce the depth by the cummulative reputation divided by 10
/// 
/// # Arguments
/// * `header` - the block header
/// * `state_root` - the state root of the chain
/// * `state_trie` - the state trie of the chain
pub fn get_difficulty_for_block(
    header: &BlockHeader, 
    reputations: &Vec<f64>,
) -> u64 {
    let cummulative_reputation: f64 = reputations.iter().filter(
        |&&rep| rep >= POR_INCLUSION_MINIMUM
    ).sum();

    if cummulative_reputation > POR_THRESHOLD {
        // if the cummulative reputation is above the threshold, we use the depth to determine difficulty
        // reduce the depth argument. -1 depth for every 10 reputation points
        return get_base_difficulty_from_depth(min(1, header.depth - (cummulative_reputation / 10.0) as u64));
    }
    get_base_difficulty_from_depth(header.depth)
}

pub async fn mine(
    block: &mut Block, 
    address: StdByteArray,
    state_root: StdByteArray,
    reputations: Vec<f64>,
    abort_signal: Option<Receiver<u64>>, 
    mut hash_function: impl HashFunction
){
    // the block is already pupulated
    block.header.nonce = 0;
    block.header.miner_address = Some(address);
    block.header.state_root = Some(state_root);
    loop {
        match block.header.hash(&mut hash_function){
            Ok(hash) => {
                let difficulty = get_difficulty_for_block(&block.header, &reputations);
                if is_valid_hash(difficulty, &hash) {
                    block.hash = Some(hash);
                    break;
                }
            },
            Err(_) => {
                panic!("Hashing failed");
            }
        }
        if let Some(ref signal) = abort_signal{
            if let Ok(d) = signal.try_recv() {
                // if we receive a signal to abort, we stop mining
                if d == block.header.depth {return;}
            }
        }
        block.header.nonce += 1;
    }
}