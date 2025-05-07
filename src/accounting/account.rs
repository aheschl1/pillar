use std::{cmp::Ordering, collections::HashMap, sync::{Arc, Mutex}};

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
