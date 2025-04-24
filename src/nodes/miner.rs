use std::sync::Arc;

use rand_core::le;
use tokio::sync::Mutex;

use crate::{primitives::block::Block, crypto::hashing::{HashFunction, Hashable}};

use super::Node;

pub struct Miner {
    pub node: Arc<Mutex<Node>>,
}

impl Miner{

    /// Creates a new miner instance
    /// Takes ownership of the node
    pub fn new(node: Node) -> Result<Self, std::io::Error> {
        let transaction_pool = &node.transaction_pool;
        if let Some(_) = transaction_pool{
            return Ok(Miner {
                node: Arc::new(Mutex::new(node)),
            });
        }else{
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Transaction pool not found",
            ));
        }
    }

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

    pub async fn mine(&self, block: &mut Block, hash_function: &mut impl HashFunction){
        // the block is already pupulated
        block.header.nonce = 0;
        block.header.miner_address = Some(*self.node.lock().await.public_key);
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

    /// monitors the nodes transaction pool
    pub async fn monitor_pool(&mut self) {
        // monitor the pool for transactions
        loop {
            
        }
    }
}
