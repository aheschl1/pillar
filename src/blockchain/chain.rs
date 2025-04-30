use std::collections::{HashMap, HashSet};

use ed25519::Signature;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

use crate::{
    crypto::hashing::{DefaultHash, HashFunction},
    primitives::{
        block::{Block, BlockHeader},
        transaction::Transaction,
    }, protocol::chain::get_genesis_block,
};

use super::account::AccountManager;

/// Represents the state of the blockchain, including blocks, accounts, and chain parameters.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Chain {
    /// The blocks in the chain.
    pub blocks: HashMap<[u8; 32], Block>,
    /// The current depth (number of blocks) in the chain.
    pub depth: u64,
    /// the block at the deepest depth
    pub deepest_hash: [u8; 32],
    // track the leaves
    pub leaves: HashSet<[u8; 32]>,
    /// The account manager for tracking account balances and nonces.
    #[serde(skip)]
    account_manager: AccountManager,
}

impl Chain {
    /// Creates a new blockchain with a genesis block.
    pub fn new_with_genesis() -> Self {
        let genesis_block = get_genesis_block();
        let mut blocks = HashMap::new();
        let genisis_hash = genesis_block.hash.unwrap();
        let mut leaves = HashSet::new();
        leaves.insert(genisis_hash);

        blocks.insert(genisis_hash, genesis_block);
        Chain {
            blocks: blocks,
            depth: 1,
            deepest_hash: genisis_hash,
            leaves: leaves,
            account_manager: AccountManager::new(),
        }
    }
    
    /// Validates the structure and metadata of a block.
    fn validate_block(&self, block: &Block) -> bool {
        // check hash validity
        if block.hash.is_none() {
            return false;
        }
        if !block.header.validate(block.hash.unwrap(), &mut DefaultHash::new()) {
            return false;
        }
        // check the previous hash exists
        let previous_hash = block.header.previous_hash;
        let previous_block = self.blocks.get(&previous_hash);
        let valid = match previous_block {
            Some(last_block) => (last_block.header.depth + 1 == block.header.depth) && (block.header.timestamp >= last_block.header.timestamp),
            None => false
        };
        if !valid {
            return false;
        }
        true
    }

    /// Ensures that all transactions in a block are valid and do not exceed available funds.
    fn validate_transaction_set(&self, transactions: &Vec<Transaction>) -> bool {
        // we need to make sure that there are no duplicated nonce values under the same user
        let per_user: HashMap<[u8; 32], Vec<&Transaction>> =
            transactions
                .into_iter()
                .fold(HashMap::new(), |mut acc, tx| {
                    acc.entry(tx.header.sender) // assuming this gives you the [u8; 32] key
                        .or_default()
                        .push(tx);
                    acc
                });
        for (user, transactions) in per_user.iter() {
            let account = self.account_manager.get_account(user);
            if account.is_none() {
                return false;
            }
            // return true;
            let total_sum: u64 = transactions.iter().map(|t| t.header.amount).sum();
            if account.unwrap().lock().unwrap().balance < total_sum {
                return false;
            }
            // now validate each individual transaction
            for transaction in transactions {
                let result = self.validate_transaction(transaction);
                if !result {
                    return false;
                }
            }
        }

        true
    }

    /// Find the longest existing fork in the chain.
    pub fn get_top_block(&self) -> Option<&Block>{
        // we use the deepest hash as the top block
        self.blocks.get(&self.deepest_hash)
    }

    pub fn get_block(&self, hash: &[u8; 32]) -> Option<&Block> {
        self.blocks.get(hash)
    }

    pub fn get_block_headers(&self) -> Vec<BlockHeader> {
        self.blocks
            .values()
            .map(|block| block.header.clone())
            .collect()
    }

    /// Validates an individual transaction for correctness.
    ///
    /// Checks:
    /// 1. The transaction is signed by the sender.
    /// 2. The transaction hash is valid.
    /// 3. The sender has sufficient balance.
    /// 4. The nonce matches the sender's expected value.
    fn validate_transaction(&self, transaction: &Transaction) -> bool {
        let sender = transaction.header.sender;
        let signature = transaction.signature;
        // check for signature
        let validating_key: VerifyingKey = VerifyingKey::from_bytes(&sender).unwrap();
        let signing_validity = match signature {
            Some(sig) => {
                let signature = Signature::from_bytes(&sig);
                validating_key
                    .verify_strict(&transaction.hash, &signature)
                    .is_ok()
            }
            None => false,
        };
        if !signing_validity {
            return false;
        }
        // check the hash
        if transaction.hash != transaction.header.hash(&mut DefaultHash::new()) {
            return false;
        }
        // verify balance
        let account = self.account_manager.get_account(&sender);
        if account.is_none() {
            return false;
        }
        let account = account.clone().unwrap();
        if account.lock().unwrap().balance < transaction.header.amount {
            return false;
        } // @todo: If there are multiple transactions of the same sender in a block, we need to check if the balance is enough for all of them
        // check nonce
        if transaction.header.nonce != account.lock().unwrap().nonce {
            return false;
        }
        return true;
    }

    /// Verifies the validity of a block, including its transactions and metadata.
    pub fn verify_block(&self, block: &Block) -> bool {
        let mut result = self.validate_block(block);
        result = result && self.validate_transaction_set(&block.transactions);
        result
    }

    /// Call this only after a block has been verified
    fn settle_new_block(&mut self, block: Block){
        self.account_manager.update_from_block(&block);
        self.blocks.insert(block.hash.unwrap(), block.clone());
        self.leaves.remove(&block.header.previous_hash);
        self.leaves.insert(block.hash.unwrap());
        // update the depth - the depth of this block is checked in the verification
        // perhaps this is a fork deeper in the chain, so we do not always update 
        if block.header.depth > self.depth {
            self.deepest_hash = block.hash.unwrap();
            self.depth = block.header.depth;
        }
    }

    /// Adds a new block to the chain if it is valid.
    ///
    /// # Arguments
    ///
    /// * `block` - The block to be added.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the block is successfully added.
    /// * `Err(std::io::Error)` if the block is invalid.
    pub fn add_new_block(&mut self, block: Block) -> Result<(), std::io::Error> {
        if self.verify_block(&block) {
            self.settle_new_block(block);
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Block is not valid",
            ))
        }
    } 

    pub fn trim(&mut self){
        let mut seen = HashMap::<[u8; 32], [u8; 32]>::new(); // node: leaf leading there
        let mut forks_to_kill = HashSet::<[u8; 32]>::new();
        let mut sorted_leaves : Vec::<&[u8; 32]> = self.leaves.iter().collect();
        // visit deepest leaves first so that we can look back at deeper forks later
        sorted_leaves.sort_by_key(|x| -(self.blocks[*x].header.depth as i64)); 
        for leaf in sorted_leaves{
            // iterate backwards, marking each node
            // if the node is already seen, then check the leaf that saw it - can they coexist?
            let current_fork_depth = self.blocks[leaf].header.depth;
            let mut current_node = self.blocks.get(leaf);
            while let Some(node) = current_node { // while there is a prevvious block
                if seen.contains_key(&node.hash.unwrap()){ // already seen
                    let fork = &seen[&node.hash.unwrap()]; // the biggest fork off so far 
                    let fork = self.blocks.get(fork).unwrap();
                    if fork.header.depth >= current_fork_depth + 10{
                        // kill this current fork from the leaf
                        forks_to_kill.insert(leaf.clone());
                        break; // leave this fork early - everything downstream has been marked, and we kill eitherway
                    }
                }else{
                    // indicate seen
                    seen.insert(node.hash.unwrap(), *leaf);
                }
                current_node = self.blocks.get(&node.header.previous_hash);
            }
        }
        // actully trim
        let mut nodes_to_remove = Vec::new();
        for fork in forks_to_kill.iter(){
            let mut current_node = self.blocks.get(fork);
            // update leaves
            self.leaves.remove(fork);
            // collect nodes to remove until the seen is no longer the fork
            while let Some(node) = current_node {
                if seen[&node.hash.unwrap()] == *fork {
                    nodes_to_remove.push(node.hash.unwrap());
                }else{
                    // we have reached the end of this fork
                    break;
                }
                current_node = self.blocks.get(&node.header.previous_hash);
            }
        }
        // perform removal after collecting nodes
        for hash in nodes_to_remove {
            self.blocks.remove(&hash);
        }
    }
}


#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;

    use super::*;
    use crate::blockchain::account::Account;
    use crate::primitives::transaction::{self, Transaction};
    use crate::crypto::hashing::DefaultHash;
    use crate::protocol::pow::mine;

    #[test]
    fn test_chain_creation() {
        let chain = Chain::new_with_genesis();
        assert_eq!(chain.depth, 1);
        assert_eq!(chain.blocks.len(), 1);
        assert_eq!(chain.leaves.len(), 1);
    }

    #[tokio::test]
    async fn test_chain_add_block() {
        let mut chain = Chain::new_with_genesis();

        let signing_key = SigningKey::generate(&mut OsRng);
        // public
        let sender = signing_key.verifying_key().to_bytes();

        let mut trans = Transaction::new(sender, [0;32], 0, 0, 0, &mut DefaultHash::new());
        trans.sign(&signing_key).unwrap();
        let mut block = Block::new(
            chain.deepest_hash, 
            0, 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            vec![
                trans
            ], 
            Some([0; 32]),
            1,
            &mut DefaultHash::new()
        );
        chain.account_manager.add_account(Account::new(sender, 0));
        mine(&mut block, sender, DefaultHash::new()).await;
        println!("Block: {:?}", block);
        let result = chain.add_new_block(block);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_chain_invalid_block() {
        let mut chain = Chain::new_with_genesis();
        let signing_key = SigningKey::generate(&mut OsRng);
        // public
        let sender = signing_key.verifying_key().to_bytes();

        let mut trans = Transaction::new(sender, [0;32], 0, 0, 0, &mut DefaultHash::new());
        trans.sign(&signing_key).unwrap();

        let block = Block::new(
            [0; 32], 
            0, 
            0, 
            vec![
               trans
            ], 
            Some([0; 32]),
            0,
            &mut DefaultHash::new()
        );
        chain.account_manager.add_account(Account::new([0; 32], 0));
        let result = chain.add_new_block(block);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_trim_removes_short_fork() {
        let mut chain = Chain::new_with_genesis();
        
        let signing_key = SigningKey::generate(&mut OsRng);
        // public
        let sender = signing_key.verifying_key().to_bytes();

        chain.account_manager.add_account(Account::new(sender, 1000));

        let mut parent_hash = chain.deepest_hash;
        let genesis_hash = parent_hash.clone();
        for depth in 1..=11 {
            let mut transaction = Transaction::new(sender, [2; 32], 10, 0, depth-1, &mut DefaultHash::new());
            transaction.sign(&signing_key).unwrap();
            let mut block = Block::new(
                parent_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth,
                vec![transaction],
                Some(sender),
                depth,
                &mut DefaultHash::new(),
            );
            mine(&mut block, sender, DefaultHash::new()).await;
            parent_hash = block.hash.unwrap();
            chain.add_new_block(block).unwrap();
        }

        let long_chain_leaf = parent_hash;

        // Create shorter fork from genesis (only 1 block)
        let mut trans = Transaction::new(sender, [2; 32], 10, 0, 11, &mut DefaultHash::new());
        trans.sign(&signing_key).unwrap();
        let mut fork_block = Block::new(
            genesis_hash, // same genesis
            0,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+30,
            vec![trans],
            Some(sender),
            1,
            &mut DefaultHash::new(),
        );
        mine(&mut fork_block, sender, DefaultHash::new()).await;
        chain.add_new_block(fork_block.clone()).unwrap();

        assert!(chain.blocks.contains_key(&fork_block.hash.unwrap()));

        // Trim should remove the short fork
        chain.trim();
        assert!(!chain.blocks.contains_key(&fork_block.hash.unwrap()));
        assert!(chain.blocks.contains_key(&long_chain_leaf));
        assert!(chain.blocks.contains_key(&chain.blocks[&long_chain_leaf].header.previous_hash));
    }

    #[tokio::test]
    async fn test_trim_keeps_close_forks() {
        let mut chain = Chain::new_with_genesis();
        let signing_key = SigningKey::generate(&mut OsRng);
        // public
        let sender = signing_key.verifying_key().to_bytes();

        chain.account_manager.add_account(Account::new(sender, 1000));

        let mut parent_hash = chain.deepest_hash;
        let genesis_hash = parent_hash;

        // Build a main chain of depth 9
        for depth in 1..=9 {
            let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth;
            let mut transaction = Transaction::new(sender, [2; 32], 10, time, depth-1, &mut DefaultHash::new());
            transaction.sign(&signing_key).unwrap();
            let mut block = Block::new(
                parent_hash,
                0,
                time,
                vec![transaction],
                Some(sender),
                depth,
                &mut DefaultHash::new(),
            );
            mine(&mut block, sender, DefaultHash::new()).await;
            parent_hash = block.hash.unwrap();
            chain.add_new_block(block).unwrap();
        }

        let mut trans = Transaction::new(sender, [2; 32], 10, 0, 9, &mut DefaultHash::new());
        trans.sign(&signing_key).unwrap();
        // Add a 1-block fork off the genesis (difference = 9)
        let mut fork_block = Block::new(
            genesis_hash,
            0,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 50,
            vec![trans],
            Some(sender),
            1,
            &mut DefaultHash::new(),
        );
        mine(&mut fork_block, sender, DefaultHash::new()).await;
        let fork_hash = fork_block.hash.unwrap();
        chain.add_new_block(fork_block).unwrap();

        // This fork is <10 behind, so it should NOT be trimmed
        chain.trim();
        assert!(chain.blocks.contains_key(&fork_hash));
    }

    #[tokio::test]
    async fn test_trim_multiple_short_forks() {
        let mut chain = Chain::new_with_genesis();
        let signing_key = SigningKey::generate(&mut OsRng);
        // public
        let sender = signing_key.verifying_key().to_bytes();
        chain.account_manager.add_account(Account::new(sender, 2000));

        let mut main_hash = chain.deepest_hash;
        let genesis_hash = main_hash;

        // Extend the main chain to depth 12
        for depth in 1..=12 {
            let mut transaction = Transaction::new(sender, [2; 32], 10, 0, depth-1, &mut DefaultHash::new());
            transaction.sign(&signing_key).unwrap();
            let mut block = Block::new(
                main_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth,
                vec![transaction],
                Some(sender),
                depth,
                &mut DefaultHash::new(),
            );
            mine(&mut block, sender, DefaultHash::new()).await;
            main_hash = block.hash.unwrap();
            chain.add_new_block(block).unwrap();
        }

        // Create two short forks from genesis (depth 1)
        let mut fork_hashes = vec![];
        for offset in 0..2 {
            let mut transaction = Transaction::new(sender, [2; 32], 10, 0, offset+12, &mut DefaultHash::new());
            transaction.sign(&signing_key).unwrap();
            let mut fork_block = Block::new(
                genesis_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 20 + offset,
                vec![transaction],
                Some(sender),
                1,
                &mut DefaultHash::new(),
            );
            mine(&mut fork_block, sender, DefaultHash::new()).await;
            let hash = fork_block.hash.unwrap();
            fork_hashes.push(hash);
            chain.add_new_block(fork_block).unwrap();
        }

        // Verify they were added
        for hash in &fork_hashes {
            assert!(chain.blocks.contains_key(hash));
            assert!(chain.leaves.contains(hash));
        }

        // Run trim — both forks should be removed
        chain.trim();

        for hash in &fork_hashes {
            assert!(!chain.blocks.contains_key(hash));
            assert!(!chain.leaves.contains(hash));
        }

        assert_eq!(chain.leaves.len(), 1); // only the longest chain remains
        assert!(chain.blocks.contains_key(&main_hash));
        // check that the right number of blocks remain
        assert_eq!(chain.blocks.len(), 13); // 12 from main chain + genesis
        // actually count the number of blocks in the main chain
        let mut current_hash = main_hash;
        let mut count = 0;
        while let Some(block) = chain.blocks.get(&current_hash) {
            count += 1;
            current_hash = block.header.previous_hash;
        }
        assert_eq!(count, 13); // 12 blocks in the main chain
    }

}
