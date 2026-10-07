use std::{collections::BTreeMap, time::{Duration, Instant}};

use pillar_crypto::{hashing::DefaultHash, types::StdByteArray};
use tracing::instrument;

use crate::{blockchain::chain::Chain, primitives::{block::{Block, BlockTail}, messages::Message, transaction::Transaction}, protocol::{pow::mine, reputation::get_current_reputations_for_stampers}};

use super::{node::{supervise, Broadcaster, Node}};

pub const MAX_TRANSACTION_WAIT_TIME: u64 = 5; // seconds
pub const MAX_BLOCK_TRANSACTION_SIZE: usize = 10; // number of transactions to mine at once
/// A proposed block that has not reached the chain in this long (its stamps never came back,
/// or nobody mined it) is proposed again.
const PROPOSAL_TIMEOUT: Duration = Duration::from_secs(30);
/// A transaction that cannot go in a block for this long (a nonce gap that never fills) is dropped.
const PENDING_EXPIRY: Duration = Duration::from_secs(600);

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

    /// Serves the node, and starts mining once the node has a chain to mine on
    /// (a new node downloads it first).
    pub async fn serve(&mut self){
        self.node.miner_pool.as_ref().unwrap().activate();
        self.node.serve().await;
        let miner = self.clone();
        tokio::spawn(async move {
            loop {
                let ready = miner.node.inner.state.read().await.is_consume()
                    && miner.node.inner.chain.lock().await.is_some();
                if ready { break; }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            let pool_miner = miner.clone();
            supervise("monitor_transaction_pool", move || monitor_transaction_pool(pool_miner.clone()));
            supervise("monitor_block_pool", move || monitor_block_pool(miner.clone()));
        });
    }

}

/// Collects transactions and proposes blocks of them.
///
/// A transaction stays pending until the chain has it (the sender's nonce has passed it), so
/// none are lost when another block wins, or when a proposal never comes back. Only one
/// proposal is out at a time, and it holds, for each sender, the unbroken run of nonces that
/// starts at the sender's nonce on the chain; so whichever block wins, the next one is valid.
#[instrument(skip_all, name="Miner::monitor_transaction_pool")]
async fn monitor_transaction_pool(miner: Miner) {
    let pool = miner.node.miner_pool.as_ref().unwrap();
    // by sender then nonce; copies relayed by several peers collapse into one
    let mut pending: BTreeMap<(StdByteArray, u64), (Transaction, Instant)> = BTreeMap::new();
    // the top our proposal extends, and when it was made
    let mut in_flight: Option<(StdByteArray, Instant)> = None;
    loop {
        tracing::trace!("waiting for transactions to mine...");
        let mut arrived = vec![];
        while let Some(transaction) = pool.pop_transaction() {
            arrived.push(transaction);
        }
        if !arrived.is_empty() || !pending.is_empty() {
            let chain_lock = miner.node.inner.chain.lock().await;
            let chain = chain_lock.as_ref().expect("Miner runs only once the chain is loaded");
            let top_hash = chain.get_top_block().unwrap().header.completion.as_ref().expect("Expected complete block").hash;
            let state_root = chain.get_state_root().unwrap();
            for transaction in arrived {
                let key = (transaction.header.sender, transaction.header.nonce);
                if pending.contains_key(&key) {
                    continue; // a relayed copy
                }
                if chain.validate_transaction(&transaction, state_root).is_err() {
                    tracing::warn!("Invalid transaction received: {:?}", transaction);
                    continue;
                }
                pending.insert(key, (transaction, Instant::now()));
            }
            // drop what the chain has taken, and what has waited too long to fit
            pending.retain(|(sender, nonce), (_, since)| {
                *nonce >= chain.state_manager.get_account_or_default(sender, state_root).nonce
                    && since.elapsed() < PENDING_EXPIRY
            });
            // the chain moved past our proposal (it won, or another block did), or it is lost
            if in_flight.is_some_and(|(previous, since)| previous != top_hash || since.elapsed() >= PROPOSAL_TIMEOUT) {
                in_flight = None;
            }
            if in_flight.is_none() {
                let ready = ready_transactions(chain, &pending, state_root);
                let waited = ready.iter()
                    .filter_map(|transaction| pending.get(&(transaction.header.sender, transaction.header.nonce)))
                    .any(|(_, since)| since.elapsed().as_secs() >= MAX_TRANSACTION_WAIT_TIME);
                if !ready.is_empty() && (ready.len() >= MAX_BLOCK_TRANSACTION_SIZE || waited) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let block = Block::new(
                        top_hash,
                        0, // undefined nonce
                        now,
                        ready,
                        None, // because this is a proposition on an unmined node
                        BlockTail::default().stamps,
                        chain.depth + 1,
                        None, // undefined state
                        None, // undefined difficulty
                        &mut DefaultHash::new()
                    );
                    tracing::info!("Proposing a block of {} transactions", block.transactions.len());
                    pool.add_block_proposition(block);
                    in_flight = Some((top_hash, Instant::now()));
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The transactions that can go in the next block: for each sender, consecutive nonces from the
/// sender's nonce on the chain, while the balance covers them; at most MAX_BLOCK_TRANSACTION_SIZE.
fn ready_transactions(
    chain: &Chain,
    pending: &BTreeMap<(StdByteArray, u64), (Transaction, Instant)>,
    state_root: StdByteArray,
) -> Vec<Transaction> {
    let mut ready = vec![];
    // the sender being walked, its next nonce, and what it has left to spend
    let mut current: Option<(StdByteArray, u64, u64)> = None;
    for ((sender, nonce), (transaction, _)) in pending {
        if ready.len() >= MAX_BLOCK_TRANSACTION_SIZE {
            break;
        }
        if current.is_none_or(|(address, _, _)| address != *sender) {
            let account = chain.state_manager.get_account_or_default(sender, state_root);
            current = Some((*sender, account.nonce, account.balance));
        }
        let (_, next_nonce, balance) = current.as_mut().unwrap();
        if *nonce == *next_nonce && transaction.header.amount <= *balance {
            *next_nonce += 1;
            *balance -= transaction.header.amount;
            ready.push(*transaction);
        }
    }
    ready
}

async fn monitor_block_pool(miner: Miner) {
    let pool = miner.node.miner_pool.as_ref().unwrap();
    loop {
        // check if there is a block to mine
        if let Some(mut block) = pool.pop_mine_ready_block(){
            block.header.tail.clean(&block.header.clone()); // removes broken signatures
            let mut chain_lock = miner.node.inner.chain.lock().await;
            let chain = chain_lock.as_mut().unwrap();
            // another block got in since this one was proposed, so it could only become a fork;
            // its transactions are still pending, and go in the next proposal
            let top = chain.get_top_block().unwrap().header.completion.as_ref().expect("Expected complete block").hash;
            if block.header.previous_hash != top {
                tracing::info!("Skipping a block that was beaten before mining");
                continue;
            }
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
            let mined = mine(
                &mut block, 
                miner.node.inner.public_key,
                state_root,
                reputations,
                Some(pool.mine_abort_receiver.clone()),
                DefaultHash::new()
            ).await;
            if !mined {
                // another block at this depth reached the chain first
                tracing::info!("Stopped mining a block that was beaten");
                continue;
            }
            // after mining the block, just transmit
            let _ = miner.node.broadcast(&Message::BlockTransmission(block)).await;
        }else{
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

#[cfg(test)]
mod test{
    use std::{net::{IpAddr, Ipv4Addr}, str::FromStr};

    use pillar_crypto::hashing::DefaultHash;

    use crate::{primitives::{block::{Block, BlockTail}, transaction::Transaction}, protocol::{difficulty::MIN_DIFFICULTY, pow::mine}};
    use crate::nodes::miner::Miner;
    use super::Node;

    #[tokio::test]
    async fn test_miner(){
        let public_key = [1u8; 32];
        let private_key = [2u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8080;
        let node = Node::new(public_key, private_key, ip_address, port, vec![], true);
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
        assert_eq!(block.header.completion.as_ref().unwrap().difficulty_target, MIN_DIFFICULTY); // assuming initial difficulty is 4
        assert_ne!(block.header.completion.as_ref().unwrap().hash, [0; 32]);
    }
}