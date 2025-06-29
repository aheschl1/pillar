use std::sync::Arc;

use flume::{Receiver, Sender};

use super::{block::Block, transaction::Transaction};


#[derive(Clone)]
pub struct MinerPool {
    // receiver channel
    transaction_receiver: Receiver<Transaction>,
    // sender channel
    transaction_sender: Sender<Transaction>,
    // proposition blocks
    block_propositions_queue: Arc<lfqueue::UnboundedQueue<Block>>,
    // ready blocks
    mine_ready_blocks_queue: Arc<lfqueue::UnboundedQueue<Block>>,
    // mine abort signal
    pub mine_abort_sender: Sender<u64>,
    pub mine_abort_receiver: Receiver<u64>,
}

/// Transaction pool for now is just a vector of transactions
/// In the future, it will be a more complex structure - perhaps a max heap on the transaction fee
/// Rn, FIFO
impl MinerPool{
    pub fn new() -> Self {
        let (transaction_sender, transaction_receiver) = flume::unbounded();
        let (mine_abort_sender, mine_abort_receiver) = flume::unbounded();
        let block_propositions_queue = Arc::new(lfqueue::UnboundedQueue::new());
        let mine_ready_blocks_queue = Arc::new(lfqueue::UnboundedQueue::new());
        MinerPool {
            transaction_receiver,
            transaction_sender,
            block_propositions_queue,
            mine_ready_blocks_queue,
            mine_abort_sender,
            mine_abort_receiver,
        }
    }

    /// Adds a transaction to the pool
    pub async fn add_transaction(&self, transaction: Transaction) {
        // send the transaction to the receiver
        self.transaction_sender.send_async(transaction).await.unwrap();
    }

    /// Returns the transaction at the front of the pool
    pub async fn pop_transaction(&self) -> Option<Transaction> {
        // receive the transaction from the sender
        match self.transaction_receiver.recv_async().await {
            Ok(transaction) => Some(transaction),
            Err(_) => None,
        }
    }

    /// Returns the block at the front of the pool
    pub fn pop_block_proposition(&self) -> Option<Block> {
        self.block_propositions_queue.dequeue()
    }

    /// Adds a block to the pool
    pub fn add_block_proposition(&self, block: Block) {
        // send the block to the receiver
        self.block_propositions_queue.enqueue(block);
    }

    pub fn add_mine_ready_block(&self, block: Block) {
        // send the block to the receiver
        self.mine_ready_blocks_queue.enqueue(block);
    }
    
    pub fn pop_mine_ready_block(&self) -> Option<Block> {
        // receive the block from the sender
        self.mine_ready_blocks_queue.dequeue()
    }

}