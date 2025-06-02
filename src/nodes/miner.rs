use std::sync::Arc;


use crate::{crypto::hashing::{DefaultHash, HashFunction}, primitives::block::{Block, BlockTail}, protocol::{pow::mine, reputation::N_TRANSMISSION_SIGNATURES}};

use super::{messages::Message, node::{Broadcaster, Node}};

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
        if self.node.chain.lock().await.is_none(){
            panic!("Cannot serve the miner before initial chain download.")
        }
        // self.node.serve().await;
        // start the miner
        tokio::spawn(self.clone().monitor_transaction_pool());
        tokio::spawn(self.clone().monitor_block_pool());
    }

    async fn monitor_block_pool(self) {
        loop {
            // check if there is a block to mine
            let block = self.node.miner_pool.as_ref().unwrap().pop_ready_block();
            match block{
                Some(mut block) => {
                    block.header.miner_address = Some(*self.node.public_key);
                    block.header.tail.clean(&block.header.clone()); // removes broken signatures
                    mine(&mut block, *self.node.public_key, DefaultHash::new()).await;
                    // after mining the block, just transmit
                    // TODO this doesnt fully belong here - also handle broadcast error
                    let _ = self.node.broadcast(&Message::BlockTransmission(block)).await;
                },
                None => {
                    // no block to mine
                    // wait for a while
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    continue;
                }
            }
        }
    }

    /// monitors the nodes transaction pool
    /// Takes ownership of a copy of the miner
    /// TODO: decide how many transactions to mine at once
    async fn monitor_transaction_pool(self) {
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
                // mining time
                // mine
                let block = Block::new(
                    self.node.chain.as_ref().lock().await.as_mut().unwrap().get_top_block().unwrap().hash.unwrap(), // if it crahses, there is bug
                    0,
                    tokio::time::Instant::now().elapsed().as_secs(),
                    transactions,
                    None, // because this is a proposition on an unmined node
                    BlockTail::default().stamps,
                    self.node.chain.as_ref().lock().await.as_mut().unwrap().depth + 1,
                    &mut DefaultHash::new()
                );
                // spawn off the mining process
                // let address = *self.node.public_key;
                let pool = self.node.miner_pool.clone();
                pool.as_ref().unwrap().add_block_proposition(block);
                // tokio::spawn(async move {
                //     mine(&mut block, address, DefaultHash::new()).await;
                //     // send it back to the node
                // });
                // reset for next mine
                last_polled_at = None;
                transactions = vec![];
            }
        }
    }
}                    // self.node.chain.lock().await.add_new_block(block.clone()).await.unwrap();



#[cfg(test)]
mod test{
    use std::{net::{IpAddr, Ipv4Addr}, str::FromStr, sync::Arc};

    use crate::{crypto::hashing::{DefaultHash, HashFunction}, persistence::database::GenesisDatastore, primitives::{block::{Block, BlockTail}, pool::MinerPool, transaction::Transaction}, protocol::{pow::mine, reputation::N_TRANSMISSION_SIGNATURES}};
    use crate::nodes::miner::Miner;
    use super::Node;

    #[tokio::test]
    async fn test_miner(){
        let public_key = [1u8; 32];
        let private_key = [2u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8080;
        let node = Node::new(public_key, private_key, ip_address, port, vec![], Some(Arc::new(GenesisDatastore::new())), Some(MinerPool::new()));
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

        let mut block = Block::new(
            previous_hash, nonce, 
            timestamp, transactions, 
            miner_address, BlockTail::default().stamps,
            1, &mut hasher);

        // mine the block
        mine(&mut block, *miner.node.public_key, hasher).await;
        
        assert!(block.header.nonce > 0);
        assert!(block.header.miner_address.is_some());
        assert_eq!(block.header.previous_hash, previous_hash);
        assert_eq!(block.header.timestamp, timestamp);
        assert!(block.hash.is_some());
    }
}