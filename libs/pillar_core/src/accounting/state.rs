use std::{collections::HashMap, fmt::Debug, sync::{Arc, Mutex}};

use pillar_crypto::{merkle_trie::MerkleTrie, types::StdByteArray};

use crate::{accounting::{account::{Account, TransactionStub}, wallet::Wallet}, primitives::block::Block, protocol::difficulty::get_reward_from_depth_and_stampers};

#[derive(Clone, Default)]
pub struct StateManager{
    // The mapping from address to account
    pub state_trie: Arc<Mutex<MerkleTrie<StdByteArray, Account>>>,
    // basic wallets for local node
    pub wallets: Arc<Mutex<HashMap<StdByteArray, Wallet>>>,
}

impl Debug for StateManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateManager")
            .finish()
    }
}

impl StateManager{
    // Creates a new account manager
    pub fn new(wallets: Option<HashMap<StdByteArray, Wallet>>) -> Self {
        StateManager {
            state_trie: Arc::new(Mutex::new(MerkleTrie::new())),
            wallets: Arc::new(Mutex::new(wallets.unwrap_or_default())),
        }
    }

    pub fn get_account(&self, address: &StdByteArray, state_root: StdByteArray) -> Option<Account> {
        let state_trie = self.state_trie.lock().expect("Failed to lock state trie");
        state_trie.get(address, state_root)
    } 

    /// Updates the accounts from the block
    /// This is called when a new block is added to the chain
    /// This does NOT verify the block - VERIFY THE BLOCK FIRST
    /// This is called when a new block is added to the chain
    pub fn branch_from_block(&mut self, block: &Block) -> StdByteArray{
        // Update the accounts from the block
        assert!(block.header.state_root.is_none(), "Block state root must be None for branch_from_block");
        let mut state_updates: Vec<Account> = Vec::new();
        let state_root = block.header.previous_hash;
        let mut state_trie = self.state_trie.lock().expect("Failed to lock state trie");

        for transaction in &block.transactions {
            let mut sender = match state_trie.get(&transaction.header.sender, state_root){
                Some(account) => account,
                None => {
                    // if the sender does not exist, we create a new account with 0 balance
                    if transaction.header.amount > 0 {
                        panic!("Sender account does not exist, but transaction amount is greater than 0");
                    }
                    Account::new(transaction.header.sender, 0)                    
                }
            };
            // may need to make a new public account for the receiver under the established public key
            let mut receiver = match state_trie.get(&transaction.header.receiver, state_root){
                Some(account) => account,
                None => Account::new(transaction.header.receiver, 0),
            };
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
            // add to the state updates
            state_updates.push(sender);
            state_updates.push(receiver);
        }
        // add the miner reward. this reward will be based upon the blocks difficulty, and the number of stamps.
        let reward = get_reward_from_depth_and_stampers(block.header.depth, block.header.tail.n_stamps());
        // settle the transaction with the miner
        let miner_address = block.header.miner_address.expect("Block must have a miner address");
        let mut miner_account = match state_trie.get(&miner_address, state_root){
            Some(account) => account,
            None => {
                // if the miner account does not exist, we create a new account with 0 balance
                Account::new(miner_address, 0)
            }
        };
        miner_account.balance += reward;
        // now add to its history
        miner_account.history.push(TransactionStub {
            block_hash: block.hash.unwrap(),
            transaction_hash: [0; 32], // This is a placeholder, as the miner reward does not have a transaction hash
        });
        state_updates.push(miner_account);

        let updates = state_updates.into_iter()
            .map(|account| (account.address, account))
            .collect::<HashMap<_, _>>();
        // branch the state trie with the updates
        state_trie.branch(Some(state_root), updates).expect("Issue with branching state trie")
    }
}