use std::{collections::HashMap, rc::Rc, sync::Mutex};

use crate::primitives::block::Block;


struct Account{
    // The address of the account is the public key
    pub address: [u8; 32],
    // The balance of the account
    pub balance: u64,
    // The nonce of the account, to prevent replay attacks
    pub nonce: u64,
}

impl Account{
    // Creates a new account with the given address and balance
    pub fn new(address: [u8; 32], balance: u64) -> Self {
        Account {
            address,
            balance,
            nonce: 0,
        }
    }

    // Increases the nonce of the account
    pub fn increment_nonce(&mut self) {
        self.nonce += 1;
    }

    // Decreases the balance of the account
    pub fn decrease_balance(&mut self, amount: u64) {
        self.balance -= amount;
    }

    // Increases the balance of the account
    pub fn increase_balance(&mut self, amount: u64) {
        self.balance += amount;
    }
}

struct AccountManager{
    // The accounts in the blockchain
    pub accounts: Vec<Rc<Mutex<Account>>>,
    // The mapping from address to account
    pub address_to_account: HashMap<[u8; 32], Rc<Mutex<Account>>>,
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
    pub fn add_account(&mut self, account: Rc<Mutex<Account>>) {
        self.accounts.push(account.clone());
        self.address_to_account.insert(account.clone().lock().unwrap().address, account);
    }

    // Gets an account by its address
    pub fn get_account(&self, address: [u8; 32]) -> Option<Rc<Mutex<Account>>> {
        self.address_to_account.get(&address).cloned()
    }

    /// Updates the accounts from the block
    /// This is called when a new block is added to the chain
    /// This does NOT verify the block - VERIFY THE BLOCK FIRST
    /// This is called when a new block is added to the chain
    pub fn update_from_block(&mut self, block: &Block){
        // Update the accounts from the block
        for transaction in &block.transactions {
            let sender = self.get_account(transaction.header.sender).unwrap();
            let receiver = self.get_account(transaction.header.receiver).unwrap();
            sender.lock().unwrap().decrease_balance(transaction.header.amount);
            receiver.lock().unwrap().increase_balance(transaction.header.amount);
            sender.lock().unwrap().increment_nonce();
        }
    }
}
