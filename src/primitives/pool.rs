use std::sync::{Arc, Mutex};
use super::{block::Block, transaction::Transaction};
use crossbeam::channel;


#[derive(Debug, Clone)]
pub struct TransactionPool {
    // receiver channel
    transaction_receiver: channel::Receiver<Transaction>,
    // sender channel
    transaction_sender: channel::Sender<Transaction>,
    // ready blocks
    block_sender: channel::Sender<Block>,
    block_receiver: channel::Receiver<Block>,
}

/// Transaction pool for now is just a vector of transactions
/// In the future, it will be a more complex structure - perhaps a max heap on the transaction fee
/// Rn, FIFO
impl TransactionPool{
    pub fn new() -> Self {
        let (transaction_sender, transaction_receiver) = channel::unbounded();
        let (block_sender, block_receiver) = channel::unbounded();
        TransactionPool {
            transaction_receiver,
            transaction_sender,
            block_sender,
            block_receiver,
        }
    }

    /// Adds a transaction to the pool
    pub fn add_transaction(&self, transaction: Transaction) {
        // send the transaction to the receiver
        self.transaction_sender.send(transaction).unwrap();
    }

    /// Returns the transaction at the front of the pool
    pub fn pop_transaction(&self) -> Option<Transaction> {
        // receive the transaction from the sender
        match self.transaction_receiver.recv() {
            Ok(transaction) => Some(transaction),
            Err(_) => None,
        }
    }

    /// Returns the block at the front of the pool
    pub fn pop_block(&self) -> Option<Block> {
        // receive the block from the sender
        match self.block_receiver.recv() {
            Ok(block) => Some(block),
            Err(_) => None,
        }
    }

    /// Adds a block to the pool
    pub fn add_block(&self, block: Block) {
        // send the block to the receiver
        self.block_sender.send(block).unwrap();
    }
}