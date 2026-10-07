use std::cmp::max;

use flume::Receiver;
use pillar_crypto::{hashing::{HashFunction, Hashable}, types::StdByteArray};


use crate::primitives::block::{Block, BlockHeader, HeaderCompletion};

use super::difficulty::_get_base_difficulty_from_depth;

pub const POR_THRESHOLD: f64 = 50f64;
pub const POR_INCLUSION_MINIMUM: f64 = 1f64;
pub const POR_MINER_SHARE_DIVISOR: u64 = 2;

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
/// * `reputations` - the reputations of the stampers
pub fn get_difficulty_for_block(
    header: &BlockHeader, 
    reputations: &Vec<f64>,
) -> (u64, bool) {
    let cummulative_reputation: f64 = reputations.iter().filter(
        |&&rep| rep >= POR_INCLUSION_MINIMUM
    ).sum();

    if cummulative_reputation > POR_THRESHOLD {
        // if the cummulative reputation is above the threshold, we use the depth to determine difficulty
        // reduce the depth argument. -1 depth for every 10 reputation points, down to depth 1
        let discount = (cummulative_reputation / 10.0) as u64;
        return (_get_base_difficulty_from_depth(max(1, header.depth.saturating_sub(discount))), true);
    }
    (_get_base_difficulty_from_depth(header.depth), false)
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
    let (difficulty, _) = get_difficulty_for_block(&block.header, &reputations);
    block.header.nonce = 0;
    block.header.completion = HeaderCompletion::new(
        [255u8; 32],
        address,
        state_root,
        difficulty,
    );
    loop {
        match block.header.hash(&mut hash_function){
            Ok(hash) => {
                if is_valid_hash(difficulty, &hash) {
                    block.header.completion.as_mut().unwrap().hash = hash;
                    break;
                }
            },
            Err(_) => {
                panic!("Hashing failed");
            }
        }
        if let Some(ref signal) = abort_signal
            && let Ok(d) = signal.try_recv() {
                // if we receive a signal to abort, we stop mining
                if d == block.header.depth {return;}
            }
        block.header.nonce += 1;
    }
}

#[cfg(test)]
mod tests {
    use pillar_crypto::hashing::DefaultHash;

    use crate::primitives::{block::{Block, BlockTail}, transaction::Transaction};
    use super::{get_difficulty_for_block, _get_base_difficulty_from_depth};

    fn header_at(depth: u64) -> crate::primitives::block::BlockHeader {
        let transaction = Transaction::new([0; 32], [0; 32], 0, 0, 0, &mut DefaultHash::new());
        Block::new([0; 32], 0, 0, vec![transaction], None, BlockTail::default().stamps, depth, None, None, &mut DefaultHash::new()).header
    }

    #[test]
    fn test_reputation_discounts_depth() {
        // 60 reputation takes 6 off the depth: 1003 -> 997, back below the step at 1000
        assert_eq!(get_difficulty_for_block(&header_at(1003), &vec![60.0]), (_get_base_difficulty_from_depth(997), true));
        assert!(get_difficulty_for_block(&header_at(1003), &vec![60.0]).0 < get_difficulty_for_block(&header_at(1003), &vec![]).0);
    }

    #[test]
    fn test_reputation_discount_stops_at_depth_one() {
        // a discount larger than the depth must not underflow
        assert_eq!(get_difficulty_for_block(&header_at(3), &vec![100.0]), (_get_base_difficulty_from_depth(1), true));
    }

    #[test]
    fn test_low_reputation_gets_no_discount() {
        // below POR_THRESHOLD, and reputations under POR_INCLUSION_MINIMUM don't count
        assert_eq!(get_difficulty_for_block(&header_at(1204), &vec![40.0, 0.5]), (_get_base_difficulty_from_depth(1204), false));
    }
}
