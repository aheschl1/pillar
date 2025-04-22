use ed25519::{signature, Signature};
use ed25519_dalek::VerifyingKey;

use crate::{crypto::hashing::{HashFunction, Sha3_256Hash}, primitives::{block::Block, transaction::Transaction}};


/// For representing and storing the state of the a blockchain
struct Chain{
    /// The blocks in the chain
    blocks: Vec<Block>,
    /// The index of the next block
    depth: u64,
    /// The difficulty of the next block
    difficulty: u64
}

impl Chain{
    /// Creates a fresh chain. This controls the parameters of the chain
    pub fn new_with_genesis() -> Self {
        let genesis_block = Block::new(
            [0; 32], 
            0,
            0,
            vec![],
            0,
            None,
            &mut Sha3_256Hash::new(),
        );
        Chain {
            blocks: vec![genesis_block],
            depth: 1,
            difficulty: 4
        }
    }


    fn validate_block_header(block: &Block) -> bool {
        // Check if the block header is valid
        // This includes checking the previous hash, timestamp, and nonce
        // For now, we will just return true
        true
    }

    fn validate_block_basic(block: &Block) -> bool {
        // Check if the block is valid
        // This includes checking the transactions and the block size
        // For now, we will just return true
        true
    }

    /// Verifies a block. Checks the following conditions:
    pub fn verify_block(&self, block: &Block) -> bool {
        true
    }

    fn validate_transaction(transaction: &Transaction) -> bool {
        let sender = transaction.header.sender;
        let signature = transaction.signature;
        // check for signature
        let validating_key: VerifyingKey = VerifyingKey::from_bytes(&sender).unwrap();
        let signing_validity = match signature {
            Some(sig) => {
                let signature = Signature::from_bytes(&sig);
                validating_key.verify_strict(&transaction.hash, &signature).is_ok()
            },
            None => false,
        };
        if !signing_validity {
            return false;
        }
        // check the hash
        if transaction.hash != transaction.header.hash(&mut Sha3_256Hash::new()) {
            return false;
        }
        // verify balance
        


        return true;
    }

    /// Adds a new block to the chain
    /// Returns an error if the block is not valid
    pub fn add_new_block(&mut self, block: Block) -> Result<(), std::io::Error> {
        if self.verify_block(&block) {
            self.blocks.push(block);
            self.depth += 1;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Block is not valid",
            ))
        }
    }
}