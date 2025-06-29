use flume::Receiver;
use pillar_crypto::{hashing::{HashFunction, Hashable}, types::StdByteArray};


use crate::primitives::block::Block;

use super::difficulty::get_difficulty_from_depth;


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

pub async fn mine(
    block: &mut Block, 
    address: StdByteArray, 
    state_root: StdByteArray,
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
                if is_valid_hash(get_difficulty_from_depth(block.header.depth), &hash) {
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