use std::sync::Arc;

use flume::{Receiver, Sender};

use super::{block::Block, transaction::Transaction};


#[derive(Clone)]
pub struct MinerPool {
    // receiver channel
    transactions_queue: Arc<crossbeam_queue::SegQueue<Transaction>>,
    // proposition blocks
    block_propositions_queue: Arc<crossbeam_queue::SegQueue<Block>>,
    // ready blocks
    mine_ready_blocks_queue: Arc<crossbeam_queue::SegQueue<Block>>,
    // mine abort signal
    pub mine_abort_sender: Sender<u64>,
    pub mine_abort_receiver: Receiver<u64>,
    // every node has a pool, but only a miner empties it; until one starts, nothing is kept
    active: Arc<std::sync::atomic::AtomicBool>,
}

/// Transaction pool for now is just a vector of transactions
/// In the future, it will be a more complex structure - perhaps a max heap on the transaction fee
/// Rn, FIFO
impl Default for MinerPool {
    fn default() -> Self {
        Self::new()
    }
}

impl MinerPool{
    pub fn new() -> Self {
        let (mine_abort_sender, mine_abort_receiver) = flume::unbounded();
        let transactions_queue = Arc::new(crossbeam_queue::SegQueue::new());
        let block_propositions_queue = Arc::new(crossbeam_queue::SegQueue::new());
        let mine_ready_blocks_queue = Arc::new(crossbeam_queue::SegQueue::new());
        MinerPool {
            transactions_queue,
            block_propositions_queue,
            mine_ready_blocks_queue,
            mine_abort_sender,
            mine_abort_receiver,
            active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Called when a miner starts on this pool
    pub fn activate(&self) {
        self.active.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether a miner takes work from this pool
    pub fn is_active(&self) -> bool {
        self.active.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Adds a transaction to the pool
    pub fn add_transaction(&self, transaction: Transaction) {
        if !self.is_active() { return; } // nobody would take it out
        // send the transaction to the receiver
        self.transactions_queue.push(transaction);
    }

    /// Returns the transaction at the front of the pool
    pub fn pop_transaction(&self) -> Option<Transaction> {
        // receive the transaction from the sender
        self.transactions_queue.pop()
    }

    /// Returns the block at the front of the pool
    pub fn pop_block_proposition(&self) -> Option<Block> {
        self.block_propositions_queue.pop()
    }

    /// Adds a block to the pool
    pub fn add_block_proposition(&self, block: Block) {
        // send the block to the receiver
        self.block_propositions_queue.push(block);
    }

    pub fn add_mine_ready_block(&self, block: Block) {
        if !self.is_active() { return; } // nobody would take it out
        // send the block to the receiver
        self.mine_ready_blocks_queue.push(block);
    }
    
    pub fn pop_mine_ready_block(&self) -> Option<Block> {
        // receive the block from the sender
        self.mine_ready_blocks_queue.pop()
    }

}