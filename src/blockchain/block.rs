use sha3::{Sha3_256, Digest};

use super::transaction::Transaction;

pub struct Block{
    // header is the header of the block
    pub header: BlockHeader,
    // transactions is a vector of transactions in this block
    pub transactions: Vec<Transaction>,
}

pub struct BlockHeader{
    // previous_hash is the sha3_356 hash of the previous block in the chain
    pub previous_hash: [u8; 32],
    // merkle_root is the root hash of the transactions in this block
    pub merkle_root: [u8; 32],
    // nonce is a random number used to find a valid hash
    pub nonce: u64,
    // timestamp is the time the block was created
    pub timestamp: u64,
}

impl BlockHeader {
    pub fn new(previous_hash: [u8; 32], merkle_root: [u8; 32], nonce: u64, timestamp: u64) -> Self {
        BlockHeader {
            previous_hash,
            merkle_root,
            nonce,
            timestamp,
        }
    }
    /// Hash the block header using SHA3-256
    /// 
    /// # Returns
    /// 
    /// * The SHA3-256 hash of the block header
    pub fn hash(&self) -> [u8; 32]{
        let mut hasher = Sha3_256::new();
        hasher.update(self.previous_hash);
        hasher.update(self.merkle_root);
        hasher.update(self.nonce.to_le_bytes());
        hasher.update(self.timestamp.to_le_bytes());

        hasher.finalize().into()
    }
}

impl Block {
    /// Create a new block
    pub fn new(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
        Block {
            header,
            transactions,
        }
    }
}
