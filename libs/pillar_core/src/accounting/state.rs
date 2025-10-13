//! State management built on a Merkle trie with optional reputation tracking.
use std::{collections::HashMap, fmt::Debug};

use pillar_crypto::{merkle_trie::MerkleTrie, types::StdByteArray};

use crate::{accounting::account::Account, primitives::block::{Block, BlockHeader}, protocol::{difficulty::get_reward_from_depth_and_stampers, pow::{get_difficulty_for_block, POR_INCLUSION_MINIMUM, POR_MINER_SHARE_DIVISOR}, reputation::get_current_reputations_for_stampers_from_state}, reputation::history::NodeHistory};

/// Mapping from address to recorded node history (used for reputation).
pub type ReputationMap = HashMap<StdByteArray, NodeHistory>;

/// Coordinates state updates and provides read access to accounts.
#[derive(Clone, Default)]
pub struct StateManager{
    // The mapping from address to account
    pub state_trie: MerkleTrie<StdByteArray, Account>,
    /// mapping of reputations for peers
    pub reputations: ReputationMap,
}

impl Debug for StateManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateManager")
            .finish()
    }
}

/// Integer division rounding toward +infinity for positive integers.
fn div_up(x: u64, y: u64) -> u64 {
    if y == 0 {
        panic!("Division by zero");
    }
    x.div_ceil(y)
}

impl StateManager{
    /// Create a new state manager with an empty trie and no reputations.
    pub fn new() -> Self {
        StateManager {
            state_trie: MerkleTrie::new(),
            reputations: HashMap::new(),
        }
    }

    /// Get an account by address at a specific `state_root` (if present).
    pub fn get_account(&self, address: &StdByteArray, state_root: StdByteArray) -> Option<Account> {
        self.state_trie.get(address, state_root)
    }

    /// Get an account or a default zero-balance account for the given address.
    pub fn get_account_or_default(&self, address: &StdByteArray, state_root: StdByteArray) -> Account {
        self.state_trie.get(address, state_root).unwrap_or(Account::new(*address, 0))
    }

    /// Return all accounts reachable from `root`.
    pub fn get_all_accounts(&self, root: StdByteArray) -> Vec<Account>{
        self.state_trie.get_all(root)
    }

    /// Remove a branched root and decrement reference counts, pruning unique nodes.
    pub fn remove_branch(&mut self, root: StdByteArray){
        self.state_trie.trim_branch(root).expect("Failed to remove branch from state trie");
    }

    /// Apply a complete block to produce a new state root branch.
    ///
    /// Notes:
    /// - The block must be validated before calling this.
    /// - The previous header must be complete to provide a `state_root`.
    pub fn branch_from_block(
        &mut self,
        block: &Block,
        prev_header: &BlockHeader,
    ) -> StdByteArray {
        let miner_address = block.header.completion.as_ref().expect("Block should be complete").miner_address;
        self.branch_from_block_internal(block, prev_header, &miner_address)
    }

    /// Internal variant of `branch_from_block` where the recipient of the full
    /// mining reward can be specified (used by PoR splitting logic).
    pub fn branch_from_block_internal(
        &mut self, 
        block: &Block, 
        prev_header: &BlockHeader,
        miner_address: &StdByteArray,
    ) -> StdByteArray{
        // grab info on the stampers from the previous block
        let previous_reputations = get_current_reputations_for_stampers_from_state(
            self,
            prev_header,
            &block.header,
        );
        let (_, por_enabled) = get_difficulty_for_block(
            &block.header,
            &previous_reputations.values().cloned().collect(),
        );
        // Update the accounts from the block
        let mut state_updates: HashMap<StdByteArray, Account> = HashMap::new();
        let state_root = prev_header.completion.as_ref().expect("Previous block should be complete").state_root;
        for transaction in &block.transactions {
            let mut sender = match state_updates.get(&transaction.header.sender){
                Some(account) => account.clone(),
                None => {
                    // if the sender does not exist, we create a new account with 0 balance
                    let account = self.state_trie
                        .get(&transaction.header.sender, state_root)
                        .unwrap_or(Account::new(transaction.header.sender, 0));

                    if account.balance < transaction.header.amount {
                        panic!("Insufficient balance for transaction");
                    }

                    account
                    
                }
            };
            sender.balance -= transaction.header.amount;
            sender.nonce += 1;
            state_updates.insert(sender.address, sender);
            // may need to make a new public account for the receiver under the established public key
            let mut receiver = match state_updates.get(&transaction.header.receiver){
                Some(account) => account.clone(),
                None => {
                    self.state_trie.get(&transaction.header.receiver, state_root).unwrap_or(Account::new(transaction.header.receiver, 0))
                },
            };
            // update balances
            receiver.balance += transaction.header.amount;
            state_updates.insert(receiver.address, receiver);
        }
        // add the miner reward. this reward will be based upon the blocks difficulty, and the number of stamps.
        let reward = get_reward_from_depth_and_stampers(block.header.depth, block.header.tail.n_stamps());
        // settle the transaction with the miner
        let mut miner_account = match state_updates.get(miner_address){
            Some(account) => account.clone(),
            None => {
                // if the miner account does not exist, we create a new account with 0 balance
                self.state_trie.get(miner_address, state_root).unwrap_or(Account::new(*miner_address, 0))
            }
        };
        miner_account.balance += if !por_enabled {reward} else {div_up(reward, POR_MINER_SHARE_DIVISOR)};
        if miner_account.history.is_none(){
            miner_account.history = Some(NodeHistory::new(*miner_address));
        }
        // distribute POR shares if PoR is enabled
        if por_enabled {
            let remaining_reward = reward - div_up(reward, POR_MINER_SHARE_DIVISOR);
            let reward_stampers: Vec<(&[u8; 32], &f64)> = previous_reputations.iter().filter(|(_, rep)| {
                **rep >= POR_INCLUSION_MINIMUM
            }).collect();

            let stamper_reward = if !reward_stampers.is_empty() {
                remaining_reward / reward_stampers.len() as u64
            } else {
                0 // no stamper, no reward
            };
            for (stamper, _) in reward_stampers {
                let mut stamper_account = match state_updates.get(stamper) {
                    Some(account) => account.clone(),
                    None => {
                        self.state_trie.get(stamper, state_root).unwrap_or(Account::new(*stamper, 0))
                    }
                };
                stamper_account.balance += stamper_reward;
                if stamper_account.history.is_none() {
                    stamper_account.history = Some(NodeHistory::new(*stamper));
                }
                state_updates.insert(stamper_account.address, stamper_account);
            }
        }
        // settle reputation
        if !por_enabled{
            // do not upgrade the trust for mining in PoR mode
            let history = miner_account.history.as_mut().unwrap();
            history.settle_miner(block.header);
            state_updates.insert(*miner_address, miner_account);
        }

        for stamper in block.header.tail.get_stampers().iter(){
            let mut stamper = match state_updates.get(stamper){
                Some(account) => account.clone(),
                None => {
                    self.state_trie.get(stamper, state_root).unwrap_or(Account::new(*stamper, 0))
                }
            };
            if stamper.history.is_none(){
                stamper.history = Some(NodeHistory::new(stamper.address));
            }
            let history = stamper.history.as_mut().unwrap();
            history.settle_stampers(block.header);
            state_updates.insert(stamper.address, stamper);
        }
        // branch the state trie with the updates
        self.state_trie.branch(Some(state_root), state_updates).expect("Issue with branching state trie")
    }
}