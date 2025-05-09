use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{crypto::hashing::{DefaultHash, HashFunction}, primitives::block::BlockHeader, protocol::chain::get_genesis_block};

use super::chain::Chain;

/// chain shard is used to build up a chain given a list of block headers
/// It is responsible for the validation and construction of the chain from a new node.
/// // chain shard is just a chain of headers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainShard {
    pub headers: HashMap<[u8; 32], BlockHeader>,
    pub leaves: HashSet<[u8; 32]>
}

impl ChainShard{
    /// ensures the hashs are good, and the depths work
    pub fn validate(&self) -> bool{
        let mut genesis_found = false;
        // validate header
        for (declared_hash, header) in self.headers.iter(){
            if header.depth == 0{
                if genesis_found{
                    return false;
                }
                // make sure valid genesis
                let correct_gensis = get_genesis_block();
                if *header != correct_gensis.header{
                    return false;
                }
                genesis_found = true;
            }
            if !header.validate(
                *declared_hash,
                &mut DefaultHash::new() 
            ){
                return false;
            }

            // check the previous hashes exists
            let previous_hash = header.previous_hash;
            let previous_block = self.headers.get(&previous_hash);
            let valid = match previous_block {
                Some(last_block) => (last_block.depth + 1 == header.depth) && (header.timestamp >= last_block.timestamp),
                None => false
            };
            if !valid {
                return false;
            }
        }
        
        genesis_found // the last check
    }

    pub fn get_block(&self, hash: &[u8; 32]) -> Option<BlockHeader>{
        self.headers.get(hash).cloned()
    }

}

impl From<Chain> for ChainShard{
    fn from(mut chain: Chain) -> Self {
        chain.trim();
        let headers = 
            chain
            .blocks
            .iter()
            .map(|(hash, block)| (hash.clone(), block.header.clone())).collect::<HashMap<_, _>>();
        
        let leaves = chain.leaves;

        Self { headers, leaves }
    }
}