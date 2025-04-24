use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{primitives::block::Block, crypto::hashing::{HashFunction, Hashable}};

use super::Node;

#[derive(Clone)]
pub struct Miner {
    pub node: Arc<Mutex<Node>>,
    current_block: Option<Block>,
}

impl Miner{

    /// Creates a new miner instance
    /// Takes ownership of the node
    pub fn new(node: Node) -> Result<Self, std::io::Error> {
        let transaction_pool = &node.transaction_pool;
        if let Some(_) = transaction_pool{
            return Ok(Miner {
                node: Arc::new(Mutex::new(node)),
                current_block: None
            });
        }else{
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Transaction pool not found",
            ));
        }
    }

    /// Starts monitoring of the pool in a background process
    /// and serves the node
    pub async fn serve(&self){
        self.node.lock().await.serve();
        // start the miner
        let self_clone = self.clone();
        tokio::spawn(self_clone.monitor_pool());

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
    /// Takes ownership of a copy of the miner
    pub async fn monitor_pool(self) {
        // monitor the pool for transactions
        loop {
            
        }
    }
}


#[cfg(test)]
mod test{
    use std::{net::{IpAddr, Ipv4Addr}, str::FromStr};

    use crate::{blockchain::chain::Chain, crypto::hashing::{DefaultHash, HashFunction}, primitives::{block::Block, pool::TransactionPool, transaction::Transaction}};
    use crate::nodes::miner::Miner;
    use super::Node;

    #[tokio::test]
    async fn test_miner(){
        let public_key = [1u8; 32];
        let private_key = [2u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8080;
        let node = Node::new(public_key, private_key, ip_address, port, vec![], Chain::new_with_genesis(), Some(TransactionPool::new()));
        let miner = Miner::new(node).unwrap();
        let mut hasher = DefaultHash::new();

        // block
        let previous_hash = [3u8; 32];
        let nonce = 12345;
        let timestamp = 1622547800;
        let transactions = vec![
            Transaction::new(
                [0u8; 32], 
                [0u8; 32], 
                1, 
                timestamp, 
                0,
                &mut hasher.clone()
            )
        ];
        let difficulty = 1;
        let miner_address = None;

        let mut block = Block::new(previous_hash, nonce, timestamp, transactions, difficulty, miner_address, &mut hasher);

        // mine the block
        miner.mine(&mut block, &mut hasher).await;

        assert!(block.header.nonce > 0);
        assert!(block.header.miner_address.is_some());
        assert_eq!(block.header.previous_hash, previous_hash);
        assert_eq!(block.header.timestamp, timestamp);
        assert_eq!(block.header.difficulty, difficulty);
        assert!(block.hash.is_some());
    }
}