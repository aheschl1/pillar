use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{crypto::hashing::{DefaultHash, HashFunction, Hashable}, primitives::block::Block, protocol::pow::mine};

use super::node::Node;

#[derive(Clone)]
pub struct Miner {
    pub node: Arc<Node>
}

impl Miner{

    /// Creates a new miner instance
    /// Takes ownership of the node
    pub fn new(node: Node) -> Result<Self, std::io::Error> {
        let transaction_pool = &node.miner_pool;
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
        if self.node.chain.is_none(){
            panic!("Cannot serve the miner before initial chain download.")
        }
        self.node.serve();
        // start the miner
        let self_clone = self.clone();
        tokio::spawn(self_clone.monitor_pool());
    }

    /// monitors the nodes transaction pool
    /// Takes ownership of a copy of the miner
    /// TODO: decide how many transactions to mine at once
    async fn monitor_pool(self) {
        // monitor the pool for transactions
        let transactions_to_mine = 10;
        let max_wait_time = 10; // seconds
        let mut transactions = vec![];
        let mut last_polled_at: Option<u64> = None;
        loop {
            let transaction = self.node.miner_pool.as_ref().unwrap().pop_transaction();
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
                let mut block = Block::new(
                    self.node.chain.as_ref().unwrap().lock().await.get_top_block().unwrap().hash.unwrap(), // if it crahses, there is bug
                    0,
                    tokio::time::Instant::now().elapsed().as_secs(),
                    transactions,
                    Some(*self.node.public_key),
                    self.node.chain.as_ref().unwrap().lock().await.depth + 1,
                    &mut DefaultHash::new()
                );
                // spawn off the mining process
                let address = *self.node.public_key;
                let pool = self.node.miner_pool.clone();
                tokio::spawn(async move {
                    mine(&mut block, address, DefaultHash::new()).await;
                    // send it back to the node
                    pool.as_ref().unwrap().add_block(block);
                });
                // reset for next mine
                last_polled_at = None;
                transactions = vec![];
            }
        }
    }
}                    // self.node.chain.lock().await.add_new_block(block.clone()).await.unwrap();



#[cfg(test)]
mod test{
    use std::{net::{IpAddr, Ipv4Addr}, str::FromStr};

    use crate::{blockchain::chain::Chain, crypto::hashing::{DefaultHash, HashFunction}, primitives::{block::Block, pool::MinerPool, transaction::Transaction}, protocol::pow::mine};
    use crate::nodes::miner::Miner;
    use super::Node;

    #[tokio::test]
    async fn test_miner(){
        let public_key = [1u8; 32];
        let private_key = [2u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8080;
        let node = Node::new(public_key, private_key, ip_address, port, vec![], Some(Chain::new_with_genesis()), Some(MinerPool::new()));
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
        let miner_address = None;

        let mut block = Block::new(previous_hash, nonce, timestamp, transactions, miner_address, 1, &mut hasher);

        // mine the block
        mine(&mut block, *miner.node.public_key, hasher).await;
        
        assert!(block.header.nonce > 0);
        assert!(block.header.miner_address.is_some());
        assert_eq!(block.header.previous_hash, previous_hash);
        assert_eq!(block.header.timestamp, timestamp);
        assert!(block.hash.is_some());
    }
}