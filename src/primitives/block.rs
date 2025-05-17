use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Deserializer, Serialize};
use serde_with::{serde_as, Bytes};

use crate::crypto::hashing::{HashFunction, Hashable, DefaultHash};
use crate::crypto::merkle::{generate_proof_of_inclusion, generate_tree, verify_proof_of_inclusion, MerkleProof, MerkleTree};
use crate::crypto::signing::Signable;
use crate::protocol::difficulty::get_difficulty_from_depth;
use crate::protocol::pow::is_valid_hash;
use crate::protocol::reputation::N_TRANSMISSION_SIGNATURES;
use super::transaction::Transaction;

#[derive(Debug, Serialize, Clone)]
pub struct Block{
    // header is the header of the block
    pub header: BlockHeader,
    // transactions is a vector of transactions in this block
    pub transactions: Vec<Transaction>,
    // hash is the sha3_256 hash of the block header - is none if it hasnt been mined
    pub hash: Option<[u8; 32]>,
    // tail is the tail of the block which can contain stamps
    pub tail: BlockTail,
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
            // tail is the tail of the block which can contain stamps
            pub tail: BlockTail,
        }

        let helper = PartialBlock::deserialize(deserializer)?;

        Ok(Block::new(
            helper.header.previous_hash,
            helper.header.nonce,
            helper.header.timestamp,
            helper.transactions,
            helper.header.miner_address,
            helper.tail.stamps,
            helper.header.depth,
            &mut DefaultHash::new()
        ))
    }
}

#[serde_as]
#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash, Serialize, Deserialize)]
pub struct Stamp{
    // the signature of the person who broadcasted the block
    #[serde_as(as = "Bytes")]
    pub signature: [u8; 64],
    // the address of the person who stamped the block
    pub address: [u8; 32]
}

impl Default for Stamp {
    fn default() -> Self {
        Stamp {
            signature: [0; 64],
            address: [0; 32]
        }
    }
}


/// A block tail tracks the signatures of people who have broadcasted the block
/// This is used for immutibility of participation reputation
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Default)]
pub struct BlockTail{
    // the signatures of the people who have broadcasted the block
    pub stamps: [Stamp; N_TRANSMISSION_SIGNATURES]
}

impl BlockTail {
    pub fn new(stamps: [Stamp; N_TRANSMISSION_SIGNATURES]) -> Self {
        BlockTail {
            stamps
        }
    }

    pub fn n_stamps(&self) -> usize {
        self.stamps.iter().filter(|s| s.signature != [0; 64]).count()
    }

    /// remove space between the signatures to ensure all empty space is at the end
    /// remove duplicate signatures
    pub fn collapse(&mut self){
        let mut seen: HashSet<[u8; 32]> = HashSet::new();
        let mut empty = VecDeque::new();
        for i in 0..N_TRANSMISSION_SIGNATURES {
            if self.stamps[i].address == [0; 32] {
                empty.push_back(i);
            }else if empty.len() > 0 {
                if seen.contains(&self.stamps[i].address) {
                    // if the address is already seen, remove it
                    self.stamps[i] = Stamp::default();
                    empty.push_back(i);
                }else{
                    seen.insert(self.stamps[i].address); // record the address
                    self.stamps.swap(i, empty.pop_front().unwrap());
                    empty.push_back(i);
                }
            }
        }
    }

    /// Stamp the block with a signature
    /// Collapses the tail to remove empty space
    /// 
    /// # Arguments
    /// * `stamp` - The stamp to add to the block
    /// 
    /// # Returns
    /// * `Ok(())` if the stamp was added successfully
    /// * `Err(std::io::Error)` if the stamp was not added successfully
    pub fn stamp(&mut self, stamp: Stamp) -> Result<(), std::io::Error>{
        self.collapse();
        if self.n_stamps() >= N_TRANSMISSION_SIGNATURES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Too many stamps"
            ));
        }
        self.stamps[self.n_stamps()] = stamp;
        Ok(())
    }

    pub fn get_stampers(&self) -> HashSet<[u8; 32]> {
        let mut stampers = HashSet::new();
        for i in 0..N_TRANSMISSION_SIGNATURES {
            if self.stamps[i].signature != [0; 64] {
                stampers.insert(self.stamps[i].address);
            }
        }
        stampers
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Eq)]
pub struct BlockHeader{
    // previous_hash is the sha3_356 hash of the previous block in the chain
    pub previous_hash: [u8; 32],
    // merkle_root is the root hash of the transactions in this block
    pub merkle_root: [u8; 32],
    // nonce is a random number used to find a valid hash
    pub nonce: u64,
    // timestamp is the time the block was created
    pub timestamp: u64,
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
        miner_address: Option<[u8; 32]>,
        depth: u64
    ) -> Self {
        BlockHeader {
            previous_hash,
            merkle_root,
            nonce,
            timestamp,
            miner_address,
            depth
        }
    }

    /// Validate header of the block
    /// Checks:
    /// * The miner is declared
    /// * The difficulty is correct
    /// * The hash is valid
    /// * The time is not too far in the future
    /// 
    /// # Arguments
    /// 
    /// * `expected_difficulty` - The expected difficulty of the block
    /// * `hasher` - A mutable instance of a type implementing the HashFunction trait
    pub fn validate(
        &self, 
        expected_hash: [u8; 32],
        hasher: &mut impl HashFunction
    ) -> bool{
        // check the miner is declared
        if self.miner_address.is_none() {
            return false;
        }
        if expected_hash != self.hash(hasher).unwrap() {
            return false;
        }
        if !is_valid_hash(get_difficulty_from_depth(self.depth), &self.hash(hasher).unwrap()) {
            return false;
        }
        // check the time is not too far in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if self.timestamp > current_time + 60 * 60 {
            // one hour margin
            return false;
        }
        true
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
        miner_address: Option<[u8; 32]>,
        stamps: [Stamp; N_TRANSMISSION_SIGNATURES],
        depth: u64,
        hasher: &mut impl HashFunction,
    ) -> Self {
        let merkle_tree = generate_tree(transactions.iter().collect(), hasher).unwrap();
        let header = BlockHeader::new(
            previous_hash, 
            merkle_tree.nodes.get(merkle_tree.root.unwrap()).unwrap().hash,
            nonce, 
            timestamp,
            miner_address,
            depth
        );
        let hash = header.hash(hasher);
        let tail = BlockTail {
            stamps: stamps
        };
        Block {
            header,
            transactions,
            hash: if hash.is_ok() {Some(hash.unwrap())} else {None},
            tail,
            merkle_tree
        }
    }

    /// Creates the proof of inclusion for a transaction in the block
    pub fn get_proof_for_transaction<T: Into<[u8; 32]>>(&self, transaction: T) -> Option<MerkleProof> {
        generate_proof_of_inclusion(
            &self.merkle_tree,
            transaction.into(),
            &mut DefaultHash::new()
        )
    }

    /// Veerifies a transaction is in the block
    pub fn validate_transaction<T: Into<[u8; 32]> + Clone>(&self, transaction: T) -> bool{
        let proof = self.get_proof_for_transaction(transaction.clone());
        if let Some(proof) = proof {
            verify_proof_of_inclusion(
                transaction.into(),
                &proof,
                self.header.merkle_root,
                &mut DefaultHash::new()
            )
        } else {
            false
        }
    }
}

impl Signable<64> for BlockHeader {
    fn get_signing_bytes(&self) -> impl AsRef<[u8]> {
        self.hash(&mut DefaultHash::new()).unwrap()
    }
    
    fn sign<const K: usize, const P: usize>(&mut self, signing_function: &mut impl crate::crypto::signing::SigFunction<K, P, 64>) -> [u8; 64] {
        signing_function.sign(self)
    }
}


impl Into<[u8; 32]> for Block{
    fn into(self) -> [u8; 32]{
        self.hash.unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tail() {
        let mut tail = BlockTail::default();
        assert_eq!(tail.n_stamps(), 0);
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        assert_eq!(tail.n_stamps(), 1);
        tail.collapse();
        assert_eq!(tail.n_stamps(), 1);
    }

    #[test]
    fn test_complex_collapse() {
        let mut tail = BlockTail::default();
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        tail.stamps[1] = Stamp {
            signature: [2; 64],
            address: [2; 32]
        };
        tail.stamps[2] = Stamp {
            signature: [3; 64],
            address: [3; 32]
        };
        tail.stamps[3] = Stamp {
            signature: [0; 64],
            address: [0; 32]
        };
        tail.collapse();
        assert_eq!(tail.n_stamps(), 3);
    }

    #[test]
    fn test_collapse_with_move(){
        let mut tail = BlockTail::default();
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        tail.stamps[1] = Stamp {
            signature: [2; 64],
            address: [2; 32]
        };
        tail.stamps[2] = Stamp {
            signature: [3; 64],
            address: [3; 32]
        };
        tail.stamps[4] = Stamp {
            signature: [4; 64],
            address: [4; 32]
        };
        tail.collapse();
        assert_eq!(tail.n_stamps(), 4);
        // make sure the empty space is at the end
        assert_eq!(tail.stamps[3].signature, [4; 64]);
    }

    #[test]
    fn test_very_compelx_collapse(){
        // test case with multiple gaps
        let mut tail = BlockTail::default();
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        tail.stamps[1] = Stamp {
            signature: [2; 64],
            address: [2; 32]
        };
        tail.stamps[3] = Stamp {
            signature: [3; 64],
            address: [3; 32]
        };
        tail.stamps[5] = Stamp {
            signature: [4; 64],
            address: [4; 32]
        };
        tail.stamps[8] = Stamp {
            signature: [5; 64],
            address: [5; 32]
        };
        // collapse the tail
        tail.collapse();
        // check the number of stamps
        assert_eq!(tail.n_stamps(), 5);
        // check the order of the stamps
        assert_eq!(tail.stamps[0].signature, [1; 64]);
        assert_eq!(tail.stamps[1].signature, [2; 64]);
        assert_eq!(tail.stamps[2].signature, [3; 64]);
        assert_eq!(tail.stamps[3].signature, [4; 64]);
        assert_eq!(tail.stamps[4].signature, [5; 64]);
        // check the empty space is at the end
        // assert_eq!(tail.stamps[5].signature, [0; 64]);
        // assert_eq!(tail.stamps[6].signature, [0; 64]);
    }
}