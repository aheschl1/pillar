use std::{cmp::Ordering, collections::HashMap, hash::Hash, sync::{Arc, Mutex}};

use serde::{Deserialize, Serialize};

use crate::primitives::block::Block;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionStub{
    // The block hash of the block that created this account
    pub block_hash: [u8; 32],
    // The transaction hash of the transaction that created this account
    pub transaction_hash: [u8; 32],
}

#[derive(Debug, PartialEq, Eq)]
pub struct Account{
    // The address of the account is the public key
    pub address: [u8; 32],
    // The balance of the account
    pub balance: u64,
    // The nonce of the account, to prevent replay attacks
    pub nonce: u64,
    // a tracking of blocks/transactions that lead to this balance
    pub history: Vec<TransactionStub>, // (block hash, transaction hash)
}

impl Account{
    // Creates a new account with the given address and balance
    pub fn new(address: [u8; 32], balance: u64) -> Self {
        // for now, this placeholder will work; however, in the long run we need a coinbase account for initial distribution
        // TODO deal with coinbase
        let history = match balance.cmp(&0){
            Ordering::Equal => vec![],
            Ordering::Greater => vec![
                TransactionStub{
                    transaction_hash: [0; 32],
                    block_hash: [0; 32]
                }
            ],
            _ => panic!()
        };
        Account {
            address,
            balance,
            nonce: 0,
            history: history
        }
    }
}

#[derive(Debug, Clone)]
pub struct AccountManager{
    // The accounts in the blockchain
    pub accounts: Vec<Arc<Mutex<Account>>>,
    // The mapping from address to account
    pub address_to_account: HashMap<[u8; 32], Arc<Mutex<Account>>>,
}

impl Default for AccountManager {
    fn default() -> Self {
        AccountManager::new()
    }
}

impl AccountManager{
    // Creates a new account manager
    pub fn new() -> Self {
        AccountManager {
            accounts: vec![],
            address_to_account: HashMap::new(),
        }
    }

    // Adds a new account to the account manager
    pub fn add_account(&mut self, account: Account) -> Arc<Mutex<Account>>{
        let account = Arc::new(Mutex::new(account));
        self.accounts.push(account.clone());
        self.address_to_account.insert(account.clone().lock().unwrap().address, account.clone());
        account
    }

    // Gets an account by its address
    pub fn get_account(&self, address: &[u8; 32]) -> Option<Arc<Mutex<Account>>> {
        self.address_to_account.get(address).cloned()
    }

    /// Updates the accounts from the block
    /// This is called when a new block is added to the chain
    /// This does NOT verify the block - VERIFY THE BLOCK FIRST
    /// This is called when a new block is added to the chain
    pub fn update_from_block(&mut self, block: &Block){
        // Update the accounts from the block
        for transaction in &block.transactions {
            let sender = self.get_account(&transaction.header.sender).unwrap();
            // may need to make a new public account for the receiver under the established public key
            let receiver = match self.get_account(&transaction.header.receiver){
                Some(account) => account,
                None => self.add_account(Account::new(transaction.header.receiver, 0)),
            };
            // sender always needs to exist, or the block would not pass verification
            let mut sender = sender.lock().unwrap();
            let mut receiver = receiver.lock().unwrap();
            // update balances
            sender.balance -= transaction.header.amount;
            sender.nonce += 1;
            receiver.balance += transaction.header.amount;
            // update history
            let history = TransactionStub{
                block_hash: block.hash.unwrap(),
                transaction_hash: transaction.hash
            };
            sender.history.push(history.clone());
            receiver.history.push(history);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::block::Block;
    use crate::primitives::transaction::{Transaction, TransactionHeader};
    use crate::crypto::hashing::{DefaultHash, HashFunction};

    #[test]
    fn test_add_account() {
        let mut account_manager = AccountManager::new();
        let account = Account::new([1; 32], 100);
        let added_account = account_manager.add_account(account);

        assert_eq!(account_manager.accounts.len(), 1);
        assert_eq!(account_manager.address_to_account.len(), 1);
        assert_eq!(added_account.lock().unwrap().balance, 100);
    }

    #[test]
    fn test_get_account() {
        let mut account_manager = AccountManager::new();
        let account = Account::new([1; 32], 100);
        account_manager.add_account(account);

        let retrieved_account = account_manager.get_account(&[1; 32]);
        assert!(retrieved_account.is_some());
        assert_eq!(retrieved_account.unwrap().lock().unwrap().balance, 100);
    }

    #[test]
    fn test_update_from_block() {
        let mut account_manager = AccountManager::new();
        let sender = account_manager.add_account(Account::new([1; 32], 100));
        let receiver = account_manager.add_account(Account::new([2; 32], 0));

        let mut hasher = DefaultHash::new();
        let transaction = Transaction::new(
            [1; 32],
            [2; 32],
            50,
            0,
            0,
            &mut hasher,
        );

        let mut block = Block::new(
            [0; 32],
            0,
            0,
            vec![transaction],
            None,
            Default::default(),
            1,
            &mut hasher,
        );

        block.hash = Some([0; 32]); // Mocking the block hash for testing

        account_manager.update_from_block(&block);

        assert_eq!(sender.lock().unwrap().balance, 50);
        assert_eq!(receiver.lock().unwrap().balance, 50);
        assert_eq!(sender.lock().unwrap().nonce, 1);
        assert_eq!(sender.lock().unwrap().history.len(), 2);
        assert_eq!(receiver.lock().unwrap().history.len(), 1);
    }

    #[test]
    fn test_create_new_account_on_transaction() {
        let mut account_manager = AccountManager::new();
        let sender = account_manager.add_account(Account::new([1; 32], 100));

        let mut hasher = DefaultHash::new();
        let transaction = Transaction::new(
            [1; 32],
            [3; 32], // Receiver does not exist yet
            50,
            0,
            0,
            &mut hasher,
        );

        let mut block = Block::new(
            [0; 32],
            0,
            0,
            vec![transaction],
            None,
            Default::default(),
            1,
            &mut hasher,
        );

        block.hash = Some([0; 32]); // Mocking the block hash for testing

        account_manager.update_from_block(&block);

        let receiver = account_manager.get_account(&[3; 32]);
        assert!(receiver.is_some());
        assert_eq!(receiver.as_ref().unwrap().lock().unwrap().history.len(), 1);
        assert_eq!(receiver.unwrap().lock().unwrap().balance, 50);
        assert_eq!(sender.lock().unwrap().balance, 50);
    }
}
