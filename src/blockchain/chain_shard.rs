use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{crypto::hashing::{DefaultHash, HashFunction, Hashable}, primitives::block::BlockHeader};

use super::chain::Chain;

/// chain shard is used to build up a chain given a list of block headers
/// It is responsible for the validation and construction of the chain from a new node.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChainShard {
    headers: HashMap<[u8; 32], BlockHeader>,
    leaves: HashSet<[u8; 32]>
}

impl ChainShard{
    /// ensures the hashs are good, and the depths work
    fn verify(&self) -> bool{
        for (declared_hash, header) in self.headers.iter(){
            // check the hash is correct
            if header.hash(&mut DefaultHash::new()).unwrap() != *declared_hash {
                return false;
            }
        }
        true
    }
}

impl From<Chain> for ChainShard{
    fn from(chain: Chain) -> Self {
        let headers = 
            chain
            .blocks
            .iter()
            .map(|(hash, block)| (hash.clone(), block.header.clone())).collect::<HashMap<_, _>>();
        
        let leaves = chain.leaves;

        Self { headers, leaves }
    }
}