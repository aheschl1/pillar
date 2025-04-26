use serde::{Deserialize, Deserializer, Serialize};

use crate::crypto::hashing::{HashFunction, Hashable, DefaultHash};
use crate::crypto::merkle::{generate_proof_of_inclusion, generate_tree, verify_proof_of_inclusion, MerkleProof, MerkleTree};
use super::transaction::Transaction;

#[derive(Debug, Serialize, Clone)]
pub struct Block{
    // header is the header of the block
    pub header: BlockHeader,
    // transactions is a vector of transactions in this block
    pub transactions: Vec<Transaction>,
    // hash is the sha3_256 hash of the block header - is none if it hasnt been mined
    pub hash: Option<[u8; 32]>,
    // the merkle tree
    #[serde(skip)]
    pub merkle_tree: MerkleTree,
}

impl<'de> Deserialize<'de> for Block {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct PartialBlock {
            // header is the header of the block
            pub header: BlockHeader,
            // transactions is a vector of transactions in this block
            pub transactions: Vec<Transaction>,
            // hash is the sha3_256 hash of the block header - is none if it hasnt been mined
            pub hash: Option<[u8; 32]>,
        }

        let helper = PartialBlock::deserialize(deserializer)?;

        Ok(Block::new(
            helper.header.previous_hash,
            helper.header.nonce,
            helper.header.timestamp,
            helper.transactions,
            helper.header.difficulty,
            helper.header.miner_address,
            helper.header.depth,
            &mut DefaultHash::new()
        ))
    }
}

#[derive(Debug, Serialize, Deserialize)]
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
    pub miner_address: Option<[u8; 32]>,
    // the depth is a depth of the block in the chain
    pub depth: u64
}

impl Clone for BlockHeader {
    fn clone(&self) -> Self {
        BlockHeader {
            previous_hash: self.previous_hash,
            merkle_root: self.merkle_root,
            nonce: self.nonce,
            timestamp: self.timestamp,
            difficulty: self.difficulty,
            miner_address: self.miner_address,
            depth: self.depth
        }
    }
}

impl BlockHeader {
    pub fn new(
        previous_hash: [u8; 32], 
        merkle_root: [u8; 32], 
        nonce: u64, timestamp: u64,
        difficulty: u64,
        miner_address: Option<[u8; 32]>,
        depth: u64
    ) -> Self {
        BlockHeader {
            previous_hash,
            merkle_root,
            nonce,
            timestamp,
            difficulty,
            miner_address,
            depth
        }
    }
}

impl Hashable for BlockHeader {
    /// Hash the block header using SHA3-256
    /// 
    /// # Returns
    /// 
    /// * The SHA3-256 hash of the block header
    fn hash(&self, hash_function: &mut impl HashFunction) -> Result<[u8; 32], std::io::Error>{
        if let None = self.miner_address {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Miner address is not set"
            ));
        }
        hash_function.update(self.previous_hash);
        hash_function.update(self.merkle_root);
        hash_function.update(self.miner_address.unwrap());
        hash_function.update(self.nonce.to_le_bytes());
        hash_function.update(self.timestamp.to_le_bytes());
        hash_function.update(self.difficulty.to_le_bytes());
        hash_function.update(self.depth.to_le_bytes());
        Ok(hash_function.digest().unwrap())
    }
}

impl Block {
    /// Create a new block
    pub fn new(
        previous_hash: [u8; 32],
        nonce: u64,
        timestamp: u64,
        transactions: Vec<Transaction>,
        difficulty: u64,
        miner_address: Option<[u8; 32]>,
        depth: u64,
        hasher: &mut impl HashFunction,
    ) -> Self {
        let merkle_tree = generate_tree(transactions.iter().collect(), hasher).unwrap();
        let header = BlockHeader::new(
            previous_hash, 
            merkle_tree.root.clone().unwrap().lock().unwrap().hash,
            nonce, 
            timestamp,
            difficulty,
            miner_address,
            depth
        );
        let hash = header.hash(hasher);
        Block {
            header,
            transactions,
            hash: if hash.is_ok() {Some(hash.unwrap())} else {None}, 
            merkle_tree
        }
    }

    /// Creates the proof of inclusion for a transaction in the block
    pub fn get_proof_for_transaction(&self, transaction: &Transaction) -> Option<MerkleProof> {
        generate_proof_of_inclusion(
            &self.merkle_tree,
            &transaction,
            &mut DefaultHash::new()
        )
    }

    /// Veerifies a transaction is in the block
    pub fn validate_transaction(&self, transaction: &Transaction) -> bool{
        let proof = self.get_proof_for_transaction(transaction);
        if let Some(proof) = proof {
            verify_proof_of_inclusion(
                &transaction,
                &proof,
                self.header.merkle_root,
                &mut DefaultHash::new()
            )
        } else {
            false
        }
    }
}
