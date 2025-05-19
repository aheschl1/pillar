use std::sync::{Arc, Mutex};
use flume::{Receiver, Sender};

use super::{block::Block, transaction::Transaction};


#[derive(Debug, Clone)]
pub struct MinerPool {
    // receiver channel
    transaction_receiver: Receiver<Transaction>,
    // sender channel
    transaction_sender: Sender<Transaction>,
    // proposition blocks
    block_proposition_sender: Sender<Block>,
    block_poroposition_receiver: Receiver<Block>,
    // ready blocks
    block_ready_sender: Sender<Block>,
    block_ready_receiver: Receiver<Block>,
}

/// Transaction pool for now is just a vector of transactions
/// In the future, it will be a more complex structure - perhaps a max heap on the transaction fee
/// Rn, FIFO
impl MinerPool{
    pub fn new() -> Self {
        let (transaction_sender, transaction_receiver) = flume::unbounded();
        let (block_sender, block_receiver) = flume::unbounded();
        let (block_ready_sender, block_ready_receiver) = flume::unbounded();
        MinerPool {
            transaction_receiver,
            transaction_sender,
            block_proposition_sender: block_sender,
            block_poroposition_receiver: block_receiver,
            block_ready_sender,
            block_ready_receiver,
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
    pub fn pop_block_preposition(&self) -> Option<Block> {
        // receive the block from the sender
        match self.block_poroposition_receiver.recv() {
            Ok(block) => Some(block),
            Err(_) => None,
        }
    }

    /// Adds a block to the pool
    pub fn add_block_proposition(&self, block: Block) {
        // send the block to the receiver
        self.block_proposition_sender.send(block).unwrap();
    }

    pub fn add_ready_block(&self, block: Block) {
        // send the block to the receiver
        self.block_ready_sender.send(block).unwrap();
    }
    
    pub fn pop_ready_block(&self) -> Option<Block> {
        // receive the block from the sender
        match self.block_ready_receiver.recv() {
            Ok(block) => Some(block),
            Err(_) => None,
        }
    }

    pub fn ready_block_count(&self) -> usize {
        // read the block count
        self.block_poroposition_receiver.len()
    }

    pub fn proposed_block_count(&self) -> usize {
        // read the block count
        self.block_ready_receiver.len()
    }
}