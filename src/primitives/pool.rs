use std::sync::{Arc, Mutex};
use super::transaction::Transaction;


#[derive(Debug, Clone)]
pub struct TransactionPool {
    transactions: Arc<Mutex<Vec<Transaction>>>
}

/// Transaction pool for now is just a vector of transactions
/// In the future, it will be a more complex structure - perhaps a max heap on the transaction fee
/// Rn, FIFO
impl TransactionPool{
    pub fn new() -> Self {
        TransactionPool {
            transactions: Arc::new(Mutex::new(Vec::new()))
        }
    }

    /// Adds a transaction to the pool
    pub fn add_transaction(&self, transaction: Transaction) {
        let mut transactions = self.transactions.lock().unwrap();
        transactions.push(transaction);
    }

    /// Returns the transaction at the front of the pool
    pub fn pop(&mut self) -> Option<Transaction> {
        let mut transactions = self.transactions.lock().unwrap();
        if transactions.len() > 0 {
            Some(transactions.remove(0))
        } else {
            None
        }
    }   
}