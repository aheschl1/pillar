use std::collections::{HashMap, HashSet};

use pillar_crypto::{hashing::DefaultHash, signing::{DefaultVerifier, SigVerFunction}, types::StdByteArray};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    accounting::{account::Account, state::StateManager}, primitives::{block::{Block, BlockHeader}, errors::BlockValidationError, transaction::Transaction}, protocol::{chain::get_genesis_block, pow::get_difficulty_for_block, reputation::get_current_reputations_for_stampers}
};

use super::TrimmableChain;

/// Represents the state of the blockchain, including blocks, accounts, and chain parameters.
#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct Chain {
    /// The blocks in the chain.
    pub blocks: HashMap<StdByteArray, Block>,
    /// header cache TODO maybe eliminate this
    pub headers: HashMap<StdByteArray, BlockHeader>,
    /// The current depth of the tallet leaf.
    /// one block is depth 0
    pub depth: u64,
    /// the block at the deepest depth
    pub deepest_hash: StdByteArray,
    // track the leaves
    pub leaves: HashSet<StdByteArray>,
    /// The account manager for tracking account balances and nonces.
    #[serde(skip)]
    pub state_manager: StateManager,
}

impl Chain {
    /// Creates a new blockchain with a genesis block.
    pub fn new_with_genesis() -> Self {
        let state_manager = StateManager::new();
        let state_root = state_manager
            .state_trie
            .lock()
            .as_mut()
            .unwrap()
            .create_genesis([0; 32], Account::default())
            .expect("Failed to create genesis state root");

        let genesis_block = get_genesis_block(Some(state_root));
        let genisis_hash = genesis_block.hash.unwrap();
        
        let mut headers = HashMap::new();
        headers.insert(genisis_hash, genesis_block.header);
        
        let mut leaves = HashSet::new();
        leaves.insert(genisis_hash);
        
        let mut blocks = HashMap::new();
        blocks.insert(genisis_hash, genesis_block);


        Chain {
            blocks,
            depth: 0,
            deepest_hash: genisis_hash,
            leaves,
            headers,
            state_manager
        }
    }

    /// Create a chain without a genesis block.
    #[instrument(
        skip_all,
        name = "Chain::new_from_blocks",
    )]
    pub fn new_from_blocks(blocks: HashMap<StdByteArray, Block>) -> Self{
        let mut headers = HashMap::new();
        let mut deepest_hash = [0; 32];
        let mut depth = 0;

        let mut all_hashes: HashSet<StdByteArray> = HashSet::new();
        let mut seen_prevs: HashSet<StdByteArray> = HashSet::new();

        for (hash, block) in &blocks {
            headers.insert(*hash, block.header);
            all_hashes.insert(*hash);
            seen_prevs.insert(block.header.previous_hash);

            if block.header.depth > depth {
                depth = block.header.depth;
                deepest_hash = *hash;
            }
        }
        tracing::debug!("Chain created with {} blocks, deepest hash: {:?}, depth: {}", blocks.len(), deepest_hash, depth);
        // leaves is those that are not seen as previous hashes
        let leaves = all_hashes
            .difference(&seen_prevs)
            .cloned()
            .collect::<HashSet<StdByteArray>>();

        Chain {
            blocks,
            headers,
            depth,
            deepest_hash,
            leaves,
            state_manager: StateManager::new(),
        }
    }
    
    /// Validates the structure and metadata of a block.
    #[instrument(skip_all, fields(block = ?block.hash))]
    fn validate_block(&self, block: &Block) -> Result<(), BlockValidationError> {
        // check hash validity
        if block.hash.is_none() {
            tracing::info!("Block hash is None - Failing");
            return Err(BlockValidationError::MalformedBlock("Hash is not specified".into()));
        }
        // get all reputations according to previous block
        let reputations = get_current_reputations_for_stampers(self, &block.header).values().cloned().collect::<Vec<f64>>();

        let (expected_target, is_por) = get_difficulty_for_block(&block.header, &reputations);

        if block.header.completion.is_none() || expected_target != block.header.completion.unwrap().difficulty_target.get() {
            tracing::info!("Block difficulty target is invalid - Failing");
            return Err(BlockValidationError::MalformedBlock("Difficulty target does not match".into()));
        }

        if let Err(error) = block.header.validate(block.hash.unwrap(), &mut DefaultHash::new()) {
            tracing::info!("Block header is not validated - Failing");
            return Err(error);
        }
        // check the previous hash exists
        let previous_hash = block.header.previous_hash;
        let previous_block = self.blocks.get(&previous_hash);

        let valid = match previous_block {
            Some(last_block) => {
                if last_block.header.depth + 1 != block.header.depth{
                    tracing::info!("Block depth is invalid - Failing");
                    return Err(BlockValidationError::MalformedBlock("Depth does not match previous block".into()));
                } else if block.header.timestamp < last_block.header.timestamp{
                    tracing::info!("Block timestamp is invalid - Failing");
                    return Err(BlockValidationError::MalformedBlock("Timestamp is before previous block".into()));
                } else{
                    Ok(())
                }
            },
            None => {
                tracing::info!("Previous block not found: {:?}", previous_hash);
                Err(BlockValidationError::MalformedBlock("Previous block not found".into()))
            }
        };

        
        valid?;

        tracing::info!("Block is valid - Continuing");
        Ok(())
    }

    /// Ensures that all transactions in a block are valid and do not exceed available funds.
    /// 
    /// Validates:
    /// 1. No duplicate nonces for the same user.
    /// 2. Sufficient balance for all transactions.
    /// 3. Nonces are contiguous and start from the account's current nonce.
    /// 
    /// # Arguments
    /// * `transactions` - A vector of transactions to validate.
    /// * `state_root` - The state root to use for account lookups. The state should be the previous block.
    #[instrument(skip_all, fields(transactions = ?transactions.iter().map(|t|t.hash).collect::<Vec<_>>()))]
    fn validate_transaction_set(&mut self, transactions: &Vec<Transaction>, state_root: StdByteArray) -> Result<(), BlockValidationError> {
        // we need to make sure that there are no duplicated nonce values under the same user
        let per_user: HashMap<StdByteArray, Vec<&Transaction>> =
            transactions
                .iter()
                .fold(HashMap::new(), |mut acc, tx| {
                    acc.entry(tx.header.sender) // assuming this gives you the StdByteArray key
                        .or_default()
                        .push(tx);
                    acc
                });
        tracing::debug!("Per user transactions: {:?}", per_user);
        tracing::info!("Validating transaction set with {} users", per_user.len());
        for (user, transactions) in per_user.iter() {
            let account = self.state_manager.get_account(user, state_root).unwrap_or(Account::new(*user, 0));
            // return true;
            let total_sum: u64 = transactions.iter().map(|t| t.header.amount).sum();
            if account.balance < total_sum {
                tracing::info!("Account balance is insufficient for user {:?} - Failing", user);
                return Err(BlockValidationError::TransactionInsufficientBalance(account.balance));
            }
            let mut nonces = vec![];
            // now validate each individual transaction
            for transaction in transactions {
                nonces.push(transaction.header.nonce);
                let result = self.validate_transaction(transaction, state_root);
                if let Err(err) = result {
                    tracing::info!("Invalid transaction - Failing");
                    return Err(err);
                }
            }
            // nonces need tto be contiguous
            nonces.sort();
            for i in 0..nonces.len() - 1 {
                if nonces[i] + 1 != nonces[i + 1] {
                    tracing::info!("Nonces are not contiguous for user {:?} - Failing", user);
                    return Err(BlockValidationError::TransactionNonceMismatch(
                        nonces[i] + 1,
                        nonces[i + 1],
                    ));
                }
            }

            // and the first one needs to be the accounts nonce
            if nonces[0] != account.nonce {
                tracing::info!("First nonce does not match account nonce for user {:?} - Failing", user);
                return Err(BlockValidationError::TransactionNonceMismatch(
                    account.nonce,
                    nonces[0],
                ));
            }

        }

        Ok(())
    }

    /// Find the longest existing fork in the chain.
    pub fn get_top_block(&self) -> Option<&Block>{
        // we use the deepest hash as the top block
        self.blocks.get(&self.deepest_hash)
    }

    pub fn get_state_root(&self) -> Option<StdByteArray> {
        self.get_top_block().and_then(|block| block.header.completion.as_ref().map(|c| c.state_root))
    }

    pub fn get_block(&self, hash: &StdByteArray) -> Option<&Block> {
        self.blocks.get(hash)
    }

    pub fn get_block_mut(&mut self, hash: &StdByteArray) -> Option<&mut Block> {
        self.blocks.get_mut(hash)
    }

    pub fn get_block_headers(&self) -> Vec<&BlockHeader> {
        self.headers.values().collect()
    }

    /// Validates an individual transaction for correctness.
    ///
    /// Checks:
    /// 1. Signature validity.
    /// 2. Hash integrity.
    /// 3. Sufficient balance for the transaction amount.
    #[instrument(skip_all, fields(transaction = ?transaction.hash))]
    pub(crate) fn validate_transaction(&self, transaction: &Transaction, state_root: StdByteArray) -> Result<(), BlockValidationError> {
        let sender = transaction.header.sender;
        let signature = transaction.signature;
        // check for signature
        let validating_key: DefaultVerifier = DefaultVerifier::from_bytes(&sender);
        let signing_validity = match signature {
            Some(sig) => {
                // let signature = Signature::from_bytes(&sig);
                validating_key.verify(&sig, transaction)
            }
            None => false,
        };
        if !signing_validity {
            tracing::info!("Transaction signature is invalid - Failing");
            return Err(BlockValidationError::TransactionInvalidSignature);
        }
        // check the hash
        if transaction.hash != transaction.header.hash(&mut DefaultHash::new()) {
            tracing::info!("Transaction hash is invalid - Failing");
            return Err(BlockValidationError::HashMismatch(
                transaction.hash,
                transaction.header.hash(&mut DefaultHash::new()),
            ));
        }
        // verify balance
        let account = self.state_manager.get_account(&sender, state_root).unwrap_or(Account::new(sender, 0));
        if account.balance < transaction.header.amount {
            tracing::info!("Account balance is insufficient - Failing");
            return Err(BlockValidationError::TransactionInsufficientBalance(account.balance));
        } 
        Ok(())
    }

    /// Verifies the validity of a block, including its transactions and metadata.
    pub fn verify_block(&mut self, block: &Block) -> Result<(), BlockValidationError> {
        self.validate_block(block)?;
        self.validate_transaction_set(
            &block.transactions, 
            self.blocks.get(&block.header.previous_hash).unwrap().header.completion.expect("Expected completed block").state_root
        )?;
        Ok(())
    }

    /// Call this only after a block has been verified
    #[instrument(skip_all, fields(block = ?block.hash))]
    fn settle_new_block(&mut self, block: Block) -> Result<(), BlockValidationError>{
        if self.blocks.get(&block.hash.unwrap()).is_some() {
            tracing::warn!("Block with hash {:?} already exists in the chain - skipping", block.hash);
            return Ok(());
        }
        let prev_header = self.headers.get(&block.header.previous_hash).expect("Previous block header must exist");
        let new_root = self.state_manager.branch_from_block(&block, prev_header);
        // last check - is the root the same as the one in the block?
        if block.header.completion.as_ref().unwrap().state_root != new_root {
            tracing::error!("Block state root does not match the computed state root - Failing");
            self.state_manager.remove_branch(new_root);
            return Err(BlockValidationError::MalformedBlock("State root does not match".into()));
        }
        tracing::debug!("Account settled");
        self.blocks.insert(block.hash.unwrap(), block.clone());
        self.leaves.remove(&block.header.previous_hash);
        self.leaves.insert(block.hash.unwrap());
        self.headers.insert(block.hash.unwrap(), block.header);
        tracing::debug!("Block settled in chain, but need to update depth.");
        // update the depth - the depth of this block is checked in the verification
        // perhaps this is a fork deeper in the chain, so we do not always update 
        if block.header.depth > self.depth {
            tracing::info!("Chain depth expanded to {}", block.header.depth);
            self.deepest_hash = block.hash.unwrap();
            self.depth = block.header.depth;
        }
        Ok(())
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
    #[instrument(skip_all, fields(block = ?block.hash))]
    pub fn add_new_block(&mut self, block: Block) -> Result<(), BlockValidationError> {
        self.verify_block(&block)?;
        tracing::info!("Block is valid, settling...");
        self.settle_new_block(block)?;
        Ok(())
    } 
}


impl TrimmableChain for Chain {
    fn get_headers(&self) -> &HashMap<StdByteArray, BlockHeader> {
        &self.headers   
    }

    fn get_leaves_mut(&mut self) -> &mut HashSet<StdByteArray> {
        &mut self.leaves
    }

    fn remove_header(&mut self, hash: &StdByteArray) {
        self.state_manager.remove_branch(self.headers.get(hash).unwrap().completion.as_ref().unwrap().state_root);
        self.blocks.remove(hash);
    }
}

#[cfg(test)]
mod tests {
    use pillar_crypto::signing::{DefaultSigner, SigFunction, SigVerFunction, Signable};

    use super::*;
    
    use crate::primitives::block::BlockTail;
    use crate::primitives::transaction::{Transaction};
    use crate::protocol::pow::mine;

    #[test]
    fn test_chain_creation() {
        let chain = Chain::new_with_genesis();
        assert_eq!(chain.depth, 0); // the depth is depth of tallest block
        assert_eq!(chain.blocks.len(), 1);
        assert_eq!(chain.leaves.len(), 1);
    }

    #[tokio::test]
    async fn test_chain_add_block() {
        let mut chain = Chain::new_with_genesis();
        let mut signing_key = DefaultSigner::generate_random();
        // public
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut trans = Transaction::new(sender, [1;32], 0, 0, 0, &mut DefaultHash::new());
        trans.sign(&mut signing_key);
        let mut block = Block::new(
            chain.deepest_hash, 
            0, 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            vec![trans],
            None,
            BlockTail::default().stamps,
            1,
            None,
            None,
            &mut DefaultHash::new()
        );
        let prev_header = chain.headers.get(&block.header.previous_hash).expect("Previous block header not found");
        let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
        mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
        let result = chain.add_new_block(block);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_chain_invalid_block() {
        let mut chain = Chain::new_with_genesis();
        let mut signing_key = DefaultSigner::generate_random();
        // public
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut trans = Transaction::new(sender, [0;32], 0, 0, 0, &mut DefaultHash::new());
        trans.sign(&mut signing_key);

        let block = Block::new(
            [0; 32], 
            0, 
            0, 
            vec![
               trans
            ], 
            None,
            BlockTail::default().stamps,
            0,
            None,
            None,
            &mut DefaultHash::new()
        );
        let result = chain.add_new_block(block);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_trim_removes_short_fork() {
        let mut chain = Chain::new_with_genesis();
        
        let mut signing_key = DefaultSigner::generate_random();
        // public
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut parent_hash = chain.deepest_hash;
        let genesis_hash = parent_hash;
        for depth in 1..=11 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth-1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                parent_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
            mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
            parent_hash = block.hash.unwrap();
            chain.add_new_block(block).unwrap();
        }

        let long_chain_leaf = parent_hash;

        // Create shorter fork from genesis (only 1 block)
        let mut trans = Transaction::new(sender, [2; 32], 0, 0, 0, &mut DefaultHash::new());
        trans.sign(&mut signing_key);
        let mut fork_block = Block::new(
            genesis_hash, // same genesis
            0,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+30,
            vec![trans],
            None,
            BlockTail::default().stamps,
            1,
            None,
            None,
            &mut DefaultHash::new(),
        );
        let prev_header = chain.headers.get(&fork_block.header.previous_hash)
            .expect("Previous block header not found");
        let state_root = chain.state_manager.branch_from_block_internal(&fork_block, prev_header, &sender);
        mine(&mut fork_block, sender, state_root, vec![], None, DefaultHash::new()).await;
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
        let mut signing_key = DefaultSigner::generate_random();
        // public
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut parent_hash = chain.deepest_hash;
        let genesis_hash = parent_hash;

        // Build a main chain of depth 9
        for depth in 1..=9 {
            let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth;
            let mut transaction = Transaction::new(sender, [2; 32], 0, time, depth-1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                parent_hash,
                0,
                time,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
            mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
            parent_hash = block.hash.unwrap();
            chain.add_new_block(block).unwrap();
        }

        let mut trans = Transaction::new(sender, [2; 32], 0, 0, 0, &mut DefaultHash::new());
        trans.sign(&mut signing_key);
        // Add a 1-block fork off the genesis (difference = 9)
        let mut fork_block = Block::new(
            genesis_hash,
            0,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 50,
            vec![trans],
            None,
            BlockTail::default().stamps,
            1,
            None,
            None,
            &mut DefaultHash::new(),
        );
        let prev_header = chain.headers.get(&fork_block.header.previous_hash)
            .expect("Previous block header not found");
        let state_root = chain.state_manager.branch_from_block_internal(&fork_block, prev_header, &sender);
        mine(&mut fork_block, sender, state_root, vec![], None, DefaultHash::new()).await;
        let fork_hash = fork_block.hash.unwrap();
        chain.add_new_block(fork_block).unwrap();

        // This fork is <10 behind, so it should NOT be trimmed
        chain.trim();
        assert!(chain.blocks.contains_key(&fork_hash));
    }

    #[tokio::test]
    async fn test_trim_multiple_short_forks() {
        let mut chain = Chain::new_with_genesis();
        let mut signing_key = DefaultSigner::generate_random();
        // public
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut main_hash = chain.deepest_hash;
        let genesis_hash = main_hash;

        // Extend the main chain to depth 12
        for depth in 1..=12 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth-1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                main_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
            mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
            main_hash = block.hash.unwrap();
            chain.add_new_block(block).unwrap();
        }

        // Create two short forks from genesis (depth 1)
        let mut fork_hashes = vec![];
        for _ in 1..3 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, 0, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut fork_block = Block::new(
                genesis_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 20,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                1,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&fork_block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root = chain.state_manager.branch_from_block_internal(&fork_block, prev_header, &sender);
            mine(&mut fork_block, sender, state_root, vec![], None, DefaultHash::new()).await;
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
    
    #[tokio::test]
    async fn test_trim_removes_multiple_forks_of_different_lengths() {
        let mut chain = Chain::new_with_genesis();
        let mut signing_key = DefaultSigner::generate_random();
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut main_hash = chain.deepest_hash;
        let genesis_hash = main_hash;

        // Extend the main chain to depth 15
        for depth in 1..=15 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth-1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                main_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+depth,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
            mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
            main_hash = block.hash.unwrap();
            chain.add_new_block(block).unwrap();
        }

        // Create forks of different lengths
        let mut fork_hashes = vec![];
        for fork_length in 1..=3 {
            let mut parent_hash = genesis_hash;
            for depth in 1..=fork_length {
                let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth-1, &mut DefaultHash::new());
                transaction.sign(&mut signing_key);
                let mut fork_block = Block::new(
                    parent_hash,
                    0,
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth + fork_length,
                    vec![transaction],
                    None,
                    BlockTail::default().stamps,
                    depth,
                    None,
                    None,
                    &mut DefaultHash::new(),
                );
                let prev_header = chain.headers.get(&fork_block.header.previous_hash)
                    .expect("Previous block header not found");
                let state_root = chain.state_manager.branch_from_block_internal(&fork_block, prev_header, &sender);
                mine(&mut fork_block, sender, state_root, vec![], None, DefaultHash::new()).await;
                parent_hash = fork_block.hash.unwrap();
                if depth == fork_length {
                    fork_hashes.push(parent_hash);
                }
                chain.add_new_block(fork_block).unwrap();
            }
        }

        // Verify forks were added
        for hash in &fork_hashes {
            assert!(chain.blocks.contains_key(hash));
            assert!(chain.leaves.contains(hash));
        }

        // Trim the chain
        chain.trim();

        // Verify forks were removed
        for hash in &fork_hashes {
            assert!(!chain.blocks.contains_key(hash));
            assert!(!chain.leaves.contains(hash));
        }

        // Verify the main chain remains intact
        assert!(chain.blocks.contains_key(&main_hash));
        assert_eq!(chain.leaves.len(), 1);
    }

    #[tokio::test]
    async fn test_trim_keeps_forks_within_threshold() {
        let mut chain = Chain::new_with_genesis();
        let mut signing_key = DefaultSigner::generate_random();
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut main_hash = chain.deepest_hash;
        let genesis_hash = main_hash;

        // Extend the main chain to depth 15
        for depth in 1..=15 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth - 1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                main_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
            mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
            main_hash = block.hash.unwrap();
            chain.add_new_block(block).unwrap();
        }

        // Create a fork that is within the threshold (difference < 10)
        let mut fork_hash = genesis_hash;
        for depth in 1..=6 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth - 1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut fork_block = Block::new(
                fork_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth + 20,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&fork_block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root = chain.state_manager.branch_from_block_internal(&fork_block, prev_header, &sender);
            mine(&mut fork_block, sender, state_root, vec![], None, DefaultHash::new()).await;
            fork_hash = fork_block.hash.unwrap();
            chain.add_new_block(fork_block).unwrap();
        }

        // Trim the chain
        chain.trim();

        // Verify the fork was not removed
        assert!(chain.blocks.contains_key(&fork_hash));
        assert!(chain.leaves.contains(&fork_hash));

        // Verify the main chain remains intact
        assert!(chain.blocks.contains_key(&main_hash));
        assert_eq!(chain.leaves.len(), 2); // Main chain and fork
    }

    #[tokio::test]
    async fn test_trim_removes_fork_with_shared_nodes() {
        let mut chain = Chain::new_with_genesis();
        let mut signing_key = DefaultSigner::generate_random();
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut main_hash = chain.deepest_hash;
        let genesis_hash = main_hash;

        // Extend the main chain to depth 10
        for depth in 1..=15 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth - 1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                main_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
            mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
            main_hash = block.hash.unwrap();
            chain.add_new_block(block).unwrap();
        }

        // Create a fork that shares some nodes with the main chain
        let mut fork_hash = genesis_hash;
        for depth in 1..=3 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth - 1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut fork_block = Block::new(
                fork_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth + 15,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&fork_block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root = chain.state_manager.branch_from_block_internal(&fork_block, prev_header, &sender);
            mine(&mut fork_block, sender, state_root, vec![], None, DefaultHash::new()).await;
            fork_hash = fork_block.hash.unwrap();
            chain.add_new_block(fork_block).unwrap();
        }

        // Trim the chain
        chain.trim();

        // Verify the fork was removed
        assert!(!chain.blocks.contains_key(&fork_hash));
        assert!(!chain.leaves.contains(&fork_hash));

        // Verify the main chain remains intact
        assert!(chain.blocks.contains_key(&main_hash));
        assert_eq!(chain.leaves.len(), 1); // Only the main chain remains
    }
}
