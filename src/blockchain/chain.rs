use std::{cmp::max, collections::HashMap};

use ed25519::Signature;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

use crate::{
    crypto::hashing::{DefaultHash, HashFunction, Hashable},
    primitives::{
        block::{Block, BlockHeader},
        transaction::Transaction,
    },
};

use super::account::AccountManager;

/// Represents the state of the blockchain, including blocks, accounts, and chain parameters.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Chain {
    /// The blocks in the chain.
    blocks: HashMap<[u8; 32], Block>,
    /// The current depth (number of blocks) in the chain.
    pub depth: u64,
    /// the block at the deepest depth
    deepest_hash: [u8; 32],
    /// The difficulty level for mining new blocks.
    pub difficulty: u64,
    /// The account manager for tracking account balances and nonces.
    #[serde(skip)]
    account_manager: AccountManager,
}

impl Chain {
    /// Creates a new blockchain with a genesis block.
    pub fn new_with_genesis() -> Self {
        let genesis_block = Block::new(
            [0; 32], 
            0, 
            0, 
            vec![
                Transaction::new([0; 32], [0;32], 0, 0, 0, &mut DefaultHash::new())
            ], 
            0, 
            Some([0; 32]),
            0,
            &mut DefaultHash::new()
        );
        let mut blocks = HashMap::new();
        let genisis_hash = genesis_block.hash.unwrap();
        blocks.insert(genisis_hash, genesis_block);
        Chain {
            blocks: blocks,
            depth: 1,
            deepest_hash: genisis_hash,
            difficulty: 4,
            account_manager: AccountManager::new(),
        }
    }

    /// Checks if a hash meets the required difficulty level.
    fn is_valid_hash(&self, difficulty: u64, hash: &[u8; 32]) -> bool {
        // check for 'difficulty' leading 0 bits
        let mut leading_zeros: u64 = 0;
        for byte in hash.iter() {
            if *byte == 0 {
                leading_zeros += 8;
            } else {
                leading_zeros += byte.leading_zeros() as u64;
                break;
            }
        }
        leading_zeros >= difficulty
    }

    /// Validates the structure and metadata of a block.
    fn validate_block(&self, block: &Block) -> bool {
        // check hash validity
        if block.hash.is_none() {
            return false;
        }
        if block.hash.unwrap() != block.header.hash(&mut DefaultHash::new()).unwrap() {
            return false;
        }
        // check the miner is declared
        if block.header.miner_address.is_none() {
            return false;
        }
        // check the difficulty
        if block.header.difficulty != self.difficulty {
            return false;
        }
        if !self.is_valid_hash(block.header.difficulty, &block.hash.unwrap()) {
            return false;
        }
        // check the previous hash exists
        // TODO: Maybe it doesnt need to be the most recent block that is previous_hash
        let previous_hash = block.header.previous_hash;
        let previous_block = self.blocks.get(&previous_hash);
        let valid = match previous_block {
            Some(last_block) => {
                // check that the depth is correct
                last_block.header.depth + 1 != block.header.depth 
            }
            None => false
        };
        if !valid {
            return false;
        }
        // check the timestamp is greater than the previous block
        let result = match previous_block {
            Some(last_block) => {
                if block.header.timestamp <= last_block.header.timestamp {
                    return false;
                }
                true
            }
            None => false,
        };
        if !result {
            return false;
        }
        // check the time is not too far in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if block.header.timestamp > current_time + 60 * 60 {
            // one hour margin
            return false;
        }

        true
    }

    /// Ensures that all transactions in a block are valid and do not exceed available funds.
    async fn validate_transaction_set(&self, transactions: &Vec<Transaction>) -> bool {
        // we need to make sure that there are no duplicated nonce values under the same user
        let per_user: HashMap<[u8; 32], Vec<&Transaction>> =
            transactions
                .into_iter()
                .fold(HashMap::new(), |mut acc, tx| {
                    acc.entry(tx.header.sender) // assuming this gives you the [u32; 32] key
                        .or_default()
                        .push(tx);
                    acc
                });
        for (user, transactions) in per_user.iter() {
            let account = self.account_manager.get_account(user);
            if account.is_none() {
                return false;
            }
            let total_sum: u64 = transactions.iter().map(|t| t.header.amount).sum();
            if account.unwrap().lock().unwrap().balance < total_sum {
                return false;
            }
            // now validate each individual transaction
            for transaction in transactions {
                let result = self.validate_transaction(transaction).await;
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
    async fn validate_transaction(&self, transaction: &Transaction) -> bool {
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
    pub async fn verify_block(&self, block: &Block) -> bool {
        self.validate_block(block);
        self.validate_transaction_set(&block.transactions).await;
        true
    }

    /// Call this only after a block has been verified
    async fn settle_new_block(&mut self, block: Block){
        self.account_manager.update_from_block(&block).await;
        self.blocks.insert(block.hash.unwrap(), block.clone());
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
    pub async fn add_new_block(&mut self, block: Block) -> Result<(), std::io::Error> {
        if self.verify_block(&block).await {
            self.settle_new_block(block).await;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Block is not valid",
            ))
        }
    }
}
