use std::{collections::HashMap, sync::{Arc, Mutex}};


use crate::primitives::block::Block;



#[derive(Debug)]
pub struct Account{
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
    pub async fn add_account(&mut self, account: Account) -> Arc<Mutex<Account>>{
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
    pub async fn update_from_block(&mut self, block: &Block){
        // Update the accounts from the block
        for transaction in &block.transactions {
            let sender = self.get_account(&transaction.header.sender).unwrap();
            // may need to make a new public account for the receiver under the established public key
            let receiver = match self.get_account(&transaction.header.receiver){
                Some(account) => account,
                None => self.add_account(Account::new(transaction.header.receiver, 0)).await,
            };
            // sender always needs to exist, or the block would not pass verification
            sender.lock().unwrap().balance -= transaction.header.amount;
            receiver.lock().unwrap().balance += transaction.header.amount;
            sender.lock().unwrap().nonce += 1;
        }
    }
}
