use std::collections::HashSet;

use pillar_crypto::hashing::DefaultHash;
use tracing::instrument;

use crate::{primitives::{block::{Block, BlockTail, HeaderCompletion}, messages::Message}, protocol::{pow::{get_difficulty_for_block, mine}, reputation::get_current_reputations_for_stampers}};

use super::{node::{Broadcaster, Node}};

pub const MAX_TRANSACTION_WAIT_TIME: u64 = 5; // seconds
pub const MAX_BLOCK_TRANSACTION_SIZE: usize = 10; // number of transactions to mine at once

#[derive(Clone)]
pub struct Miner {
    pub node: Node
}

impl Miner{

    /// Creates a new miner instance
    /// Takes ownership of the node
    pub fn new(node: Node) -> Result<Self, std::io::Error> {
        let miner_pool = &node.miner_pool;
        if miner_pool.is_some(){
            Ok(Miner {
                node,
            })
        }else{
            Err(std::io::Error::other(
                "miner_pool pool not found",
            ))
        }
    }

    /// Starts monitoring of the pool in a background process
    /// and serves the node
    pub async fn serve(&mut self){
        if self.node.inner.chain.lock().await.is_none(){
            panic!("Cannot serve the miner before initial chain download.")
        }
        self.node.serve().await;
        // start the miner
        tokio::spawn(monitor_transaction_pool(self.clone()));
        tokio::spawn(monitor_block_pool(self.clone()));
    }

}

/// monitors the nodes transaction pool
/// Takes ownership of a copy of the miner
/// TODO: decide how many transactions to mine at once
#[instrument(skip_all, name="Miner::monitor_transaction_pool")]
async fn monitor_transaction_pool(miner: Miner) {
    // monitor the pool for transactions
    let mut transactions = HashSet::new();
    let mut last_polled_at: Option<u64> = None;
    loop {
        tracing::trace!("waiting for transactions to mine...");

        if let Some(transaction) = miner.node.miner_pool.as_ref().unwrap().pop_transaction(){
            let chain = miner.node.inner.chain.lock().await;
            if let Some(chain) = chain.as_ref() {
                // check if the transaction is valid
                if chain.validate_transaction(&transaction, chain.get_state_root().unwrap()).is_err() {
                    tracing::warn!("Invalid transaction received: {:?}", transaction);
                    continue; // skip invalid transactions
                }
            } else {
                tracing::error!("Chain is not initialized, cannot validate transaction.");
                continue; // skip if chain is not initialized
            }
            drop(chain); // cause i feel like it
            transactions.insert(transaction);
            // grab unix timestamp
            last_polled_at = Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if (last_polled_at.is_some() && now - last_polled_at.unwrap() >= MAX_TRANSACTION_WAIT_TIME) || transactions.len() >= MAX_BLOCK_TRANSACTION_SIZE {
            // mining time
            // mine
            let chain_lock = miner.node.inner.chain.lock().await;
            let chain = chain_lock.as_ref().unwrap();
            let block = Block::new(
                chain.get_top_block().unwrap().hash.unwrap(), // if it crahses, there is bug
                0, // undefined nonce
                now,
                transactions.iter().copied().collect(),
                None, // because this is a proposition on an unmined node
                BlockTail::default().stamps,
                chain.depth + 1,
                None, // undefined state
                None, // undefined difficulty
                &mut DefaultHash::new()
            );
            // spawn off the mining process
            // let address = *self.node.public_key;
            let pool = miner.node.miner_pool.clone();
            pool.as_ref().unwrap().add_block_proposition(block);
            // reset for next mine
            last_polled_at = None;
            transactions = HashSet::new();
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

async fn monitor_block_pool(miner: Miner) {
    loop {
        // check if there is a block to mine
        if let Some(mut block) = miner.node.miner_pool.as_ref().unwrap().pop_mine_ready_block(){
            block.header.tail.clean(&block.header.clone()); // removes broken signatures
            let mut chain_lock = miner.node.inner.chain.lock().await;
            let chain = chain_lock.as_mut().unwrap();
            let prev_block = chain
                .headers
                .get(&block.header.previous_hash)
                .expect("Previous header must exist");
            
            let state_root = chain
                .state_manager
                .branch_from_block_internal(&block, prev_block, &miner.node.inner.public_key);
            let reputations = get_current_reputations_for_stampers(
                chain, 
                &block.header
            ).values().cloned().collect::<Vec<f64>>();
            drop(chain_lock); // drop the lock before mining
            // the block is already pupulated
            mine(
                &mut block, 
                miner.node.inner.public_key,
                state_root,
                reputations,
                Some(miner.node.miner_pool.as_ref().unwrap().mine_abort_receiver.clone()),
                DefaultHash::new()
            ).await;
            // after mining the block, just transmit
            // TODO this doesnt fully belong here - also handle broadcast error
            let _ = miner.node.broadcast(&Message::BlockTransmission(block)).await;
        }else{
            // TODO THIS IS KIND OF ASS
            // there is a bug where if we do not yield, then the runtime locks up
            // however, this sleep also cannot be too long or else we break.
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}

#[cfg(test)]
mod test{
    use std::{net::{IpAddr, Ipv4Addr}, str::FromStr, sync::Arc};

    use pillar_crypto::hashing::DefaultHash;

    use crate::{persistence::database::GenesisDatastore, primitives::{block::{Block, BlockTail}, pool::MinerPool, transaction::Transaction}, protocol::{difficulty::MIN_DIFFICULTY, pow::mine}};
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
            1, None, None, &mut hasher);

        // mine the block
        mine(&mut block, miner.node.inner.public_key, [8; 32], vec![], None, hasher).await;
        
        assert!(block.header.nonce > 0);
        assert!(block.header.completion.is_some());
        assert_eq!(block.header.previous_hash, previous_hash);
        assert_eq!(block.header.timestamp, timestamp);
        assert_eq!(block.header.completion.unwrap().difficulty_target, MIN_DIFFICULTY); // assuming initial difficulty is 4
        assert!(block.hash.is_some());
    }
}