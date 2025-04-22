use crate::{primitives::block::{Block, BlockHeader}, crypto::hashing::{HashFunction, Hashable}};

use super::Node;

pub trait Miner {
    /// Mine a block and retrun the mined block
    fn mine(&self, block: &mut Block, hash_function: &mut impl HashFunction);
    /// Check if the hash is valid
    fn is_valid_hash(&self, difficulty: u64, hash: &[u8; 32]) -> bool;
}

impl Miner for Node{

    fn is_valid_hash(&self, difficulty: u64, hash: &[u8; 32]) -> bool {
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

    fn mine(&self, block: &mut Block, hash_function: &mut impl HashFunction){
        // the block is already pupulated
        block.header.nonce = 0;
        block.header.miner_address = Some(self.address);
        loop {
            match block.header.hash(hash_function){
                Ok(hash) => {
                    if self.is_valid_hash(block.header.difficulty, &hash) {
                        block.hash = Some(hash);
                        break;
                    }
                },
                Err(_) => {
                    panic!("Hashing failed");
                }
            }
            block.header.nonce += 1;
        }
    }
}
