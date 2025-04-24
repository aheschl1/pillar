use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{crypto::hashing::{DefaultHash, HashFunction, Hashable}, primitives::block::Block};

use super::Node;

#[derive(Clone)]
pub struct Miner {
    pub node: Arc<Node>
}

impl Miner{

    /// Creates a new miner instance
    /// Takes ownership of the node
    pub fn new(node: Node) -> Result<Self, std::io::Error> {
        let transaction_pool = &node.transaction_pool;
        if let Some(_) = transaction_pool{
            return Ok(Miner {
                node: Arc::new(node),
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
        self.node.serve();
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

    pub async fn mine(self, mut block: Block, mut hash_function: impl HashFunction){
        // the block is already pupulated
        block.header.nonce = 0;
        block.header.miner_address = Some(*self.node.public_key);
        loop {
            match block.header.hash(&mut hash_function){
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
        // add the ready queue
        self.node.transaction_pool.as_ref().unwrap().add_block(block);
    }

    /// monitors the nodes transaction pool
    /// Takes ownership of a copy of the miner
    /// TODO: decide how many transactions to mine at once
    pub async fn monitor_pool(self) {
        // monitor the pool for transactions
        let transactions_to_mine = 10;
        let max_wait_time = 10; // seconds
        let mut transactions = vec![];
        let mut last_polled_at: Option<u64> = None;
        loop {
            let transaction = self.node.transaction_pool.as_ref().unwrap().pop_transaction();
            match transaction{
                Some(transaction) => {
                    transactions.push(transaction);
                    // grab unix timestamp
                    last_polled_at = Some(tokio::time::Instant::now().elapsed().as_secs());
                },
                None => {}
            }
            if (last_polled_at.is_some() && tokio::time::Instant::now().elapsed().as_secs() - last_polled_at.unwrap() > max_wait_time) ||
                transactions.len() >= transactions_to_mine {
                // mine
                let block = Block::new(
                    self.node.chain.lock().await.get_top_block().unwrap().hash.unwrap(), // if it crahses, there is bug
                    0,
                    tokio::time::Instant::now().elapsed().as_secs(),
                    transactions,
                    self.node.chain.lock().await.difficulty,
                    Some(*self.node.public_key),
                    &mut DefaultHash::new()
                );
                // spawn off the mining process
                tokio::spawn(self.clone().mine(block, DefaultHash::new()));
                // reset for next mine
                last_polled_at = None;
                transactions = vec![];
            }
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

        let block = Block::new(previous_hash, nonce, timestamp, transactions, difficulty, miner_address, &mut hasher);

        // mine the block
        miner.clone().mine(block, hasher).await;
        
        let mined_block = miner.node.transaction_pool.as_ref().unwrap().pop_block().unwrap();
        assert!(mined_block.header.nonce > 0);
        assert!(mined_block.header.miner_address.is_some());
        assert_eq!(mined_block.header.previous_hash, previous_hash);
        assert_eq!(mined_block.header.timestamp, timestamp);
        assert_eq!(mined_block.header.difficulty, difficulty);
        assert!(mined_block.hash.is_some());
    }
}