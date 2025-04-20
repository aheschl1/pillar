use crate::crypto::hashing::{HashFunction, Hashable};
use super::transaction::Transaction;

pub struct Block{
    // header is the header of the block
    pub header: BlockHeader,
    // transactions is a vector of transactions in this block
    pub transactions: Vec<Transaction>,
    // hash is the sha3_256 hash of the block header
    pub hash: [u8; 32]
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
    // difficulty is the difficulty of the block
    pub difficulty: u64,
    // the address of the miner is the sha3_256 hash of the miner address
    pub miner_address: [u8; 32],
}

impl BlockHeader {
    pub fn new(
        previous_hash: [u8; 32], 
        merkle_root: [u8; 32], 
        nonce: u64, timestamp: u64,
        difficulty: u64,
        miner_address: [u8; 32]
    ) -> Self {
        BlockHeader {
            previous_hash,
            merkle_root,
            nonce,
            timestamp,
            difficulty,
            miner_address,
        }
    }
}

impl Hashable for BlockHeader {
    /// Hash the block header using SHA3-256
    /// 
    /// # Returns
    /// 
    /// * The SHA3-256 hash of the block header
    fn hash(&self, hash_function: &mut impl HashFunction) -> [u8; 32]{
        hash_function.update(self.previous_hash);
        hash_function.update(self.merkle_root);
        hash_function.update(self.miner_address);
        hash_function.update(self.nonce.to_le_bytes());
        hash_function.update(self.timestamp.to_le_bytes());
        hash_function.update(self.difficulty.to_le_bytes());

        hash_function.digest().unwrap()
    }
}

impl Block {
    /// Create a new block
    pub fn new(
        previous_hash: [u8; 32],
        merkle_root: [u8; 32],
        nonce: u64,
        timestamp: u64,
        transactions: Vec<Transaction>,
        difficulty: u64,
        miner_address: [u8; 32],
        hasher: &mut impl HashFunction
    ) -> Self {
        let header = BlockHeader::new(
            previous_hash, 
            merkle_root, 
            nonce, 
            timestamp,
            difficulty,
            miner_address
        );
        let hash = header.hash(hasher);
        Block {
            header,
            transactions,
            hash
        }
    }
}
