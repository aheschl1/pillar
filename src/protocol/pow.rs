use crate::{crypto::hashing::{HashFunction, Hashable}, primitives::block::Block};


pub fn is_valid_hash(difficulty: u64, hash: &[u8; 32]) -> bool {
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

pub async fn mine(block: &mut Block, address: [u8; 32], mut hash_function: impl HashFunction){
    // the block is already pupulated
    block.header.nonce = 0;
    block.header.miner_address = Some(address);
    loop {
        match block.header.hash(&mut hash_function){
            Ok(hash) => {
                if is_valid_hash(block.header.difficulty, &hash) {
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