use std::collections::{HashSet, VecDeque};

use pillar_crypto::hashing::{DefaultHash, HashFunction, Hashable};
use pillar_crypto::merkle::{generate_tree, MerkleTree};
use pillar_crypto::proofs::{generate_proof_of_inclusion, verify_proof_of_inclusion, MerkleProof};
use pillar_crypto::signing::{DefaultVerifier, SigFunction, SigVerFunction, Signable};
use pillar_crypto::types::StdByteArray;
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::{serde_as, Bytes};

use crate::primitives::errors::BlockValidationError;
use crate::protocol::difficulty::get_difficulty_from_depth;
use crate::protocol::pow::is_valid_hash;
use crate::protocol::reputation::N_TRANSMISSION_SIGNATURES;
use super::transaction::Transaction;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct Block{
    // header is the header of the block
    pub header: BlockHeader,
    // transactions is a vector of transactions in this block
    pub transactions: Vec<Transaction>,
    // hash is the sha3_256 hash of the block header - is none if it hasnt been mined
    pub hash: Option<StdByteArray>,
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
            pub _hash: Option<StdByteArray>,
        }

        let helper = PartialBlock::deserialize(deserializer)?;

        Ok(Block::new(
            helper.header.previous_hash,
            helper.header.nonce,
            helper.header.timestamp,
            helper.transactions,
            helper.header.miner_address,
            helper.header.tail.stamps,
            helper.header.depth,
            helper.header.state_root,
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
    pub address: StdByteArray
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Default, Hash)]
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
        let mut seen: HashSet<StdByteArray> = HashSet::new();
        let mut empty = VecDeque::new();
        for i in 0..N_TRANSMISSION_SIGNATURES {
            if self.stamps[i].address == [0; 32] {
                empty.push_back(i);
            }else{
                if seen.contains(&self.stamps[i].address) {
                    // if the address is already seen, remove it
                    self.stamps[i] = Stamp::default();
                    empty.push_back(i);
                } else if !empty.is_empty() {
                    self.stamps.swap(i, empty.pop_front().unwrap());
                    empty.push_back(i);
                }
                seen.insert(self.stamps[i].address); // record the address
            }
        }
    }


    /// removes any stamps with invalid signatures
    pub fn clean(&mut self, target: &impl Signable<64>) {
        for i in 0..self.n_stamps() {
            let sigver = DefaultVerifier::from_bytes(&self.stamps[i].address);
            if !sigver.verify(&self.stamps[i].signature, target) {
                self.stamps[i] = Stamp::default();
            }
        }
        self.collapse();
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

    pub fn get_stampers(&self) -> HashSet<StdByteArray> {
        let mut stampers = HashSet::new();
        for i in 0..N_TRANSMISSION_SIGNATURES {
            if self.stamps[i].signature != [0; 64] {
                stampers.insert(self.stamps[i].address);
            }
        }
        stampers
    }

    // iter stamps
    pub fn iter_stamps(&self) -> impl Iterator<Item = &Stamp> {
        self.stamps.iter().filter(|s| s.signature != [0; 64])
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Default)]
pub struct BlockHeader{
    // previous_hash is the sha3_356 hash of the previous block in the chain
    pub previous_hash: StdByteArray,
    // merkle_root is the root hash of the transactions in this block
    pub merkle_root: StdByteArray,
    // state_root is the root hash of the global state after this block
    pub state_root: Option<StdByteArray>,
    // nonce is a random number used to find a valid hash
    pub nonce: u64,
    // timestamp is the time the block was created
    pub timestamp: u64,
    // the address of the miner is the sha3_256 hash of the miner address
    pub miner_address: Option<StdByteArray>,
    // the depth is a depth of the block in the chain
    pub depth: u64,
    // tail is the tail of the block which can contain stamps
    pub tail: BlockTail,
}

impl BlockHeader {
    pub fn new(
        previous_hash: StdByteArray, 
        merkle_root: StdByteArray, 
        state_root: Option<StdByteArray>,
        nonce: u64, timestamp: u64,
        miner_address: Option<StdByteArray>,
        tail: BlockTail,
        depth: u64
    ) -> Self {
        BlockHeader {
            previous_hash,
            merkle_root,
            state_root,
            nonce,
            timestamp,
            miner_address,
            depth,
            tail
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
    /// * `expected_hash` - The expected hash of the block
    /// * `hasher` - A mutable instance of a type implementing the HashFunction trait
    pub fn validate(
        &self, 
        expected_hash: StdByteArray,
        hasher: &mut impl HashFunction
    ) -> Result<(), BlockValidationError> {
        // check the miner is declared
        if self.miner_address.is_none() {
            return Err(BlockValidationError::NoMinerAddress(*self));
        }
        if self.state_root.is_none() {
            return Err(BlockValidationError::NoStateRoot(*self));
        }
        if expected_hash != self.hash(hasher).unwrap() {
            return Err(BlockValidationError::HashMismatch(expected_hash, self.hash(hasher).unwrap()));
        }
        if !is_valid_hash(get_difficulty_from_depth(self.depth), &self.hash(hasher).unwrap()) {
            return Err(BlockValidationError::DifficultyMismatch(get_difficulty_from_depth(self.depth), *self));
        }
        // check that all the signatures work in the tail
        let tail = &mut self.tail.clone();
        tail.collapse();
        for i in 0..tail.n_stamps() {
            let stamp = tail.stamps[i];
            let sigver = DefaultVerifier::from_bytes(&stamp.address);
            if !sigver.verify(&stamp.signature, self) {
                return Err(BlockValidationError::InvalidStampSignature(stamp.address));
            }
        }


        // check the time is not too far in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if self.timestamp > current_time + 60 * 60 {
            // one hour margin
            return Err(BlockValidationError::FutureTimestamp(self.timestamp));
        }
        Ok(())
    }

    /// A hashing function that doesnt rely on any moving pieces like the miner address
    /// This is used for stamping - so that you can stamp before the miner address is set, and it doesnt change based on future stamps.
    fn hash_clean(
        &self,
        hasher: &mut impl HashFunction
    ) -> Result<StdByteArray, std::io::Error>{
        hasher.update(self.previous_hash);
        hasher.update(self.merkle_root);
        hasher.update(self.timestamp.to_le_bytes());
        hasher.update(self.depth.to_le_bytes());
        Ok(hasher.digest().unwrap())
    }
}

impl Hashable for BlockHeader {
    /// Hash the block header using SHA3-256
    /// 
    /// # Returns
    /// 
    /// * The SHA3-256 hash of the block header
    fn hash(&self, hash_function: &mut impl HashFunction) -> Result<StdByteArray, std::io::Error>{
        if self.miner_address.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Miner address is not set"
            ));
        }
        if self.state_root.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "State root is not set"
            ));
        }
        hash_function.update(self.previous_hash);
        hash_function.update(self.merkle_root);
        hash_function.update(self.miner_address.unwrap());
        hash_function.update(self.state_root.expect("Must have a state root to hash"));
        hash_function.update(self.nonce.to_le_bytes());
        hash_function.update(self.timestamp.to_le_bytes());
        hash_function.update(self.depth.to_le_bytes());
        for i in 0..N_TRANSMISSION_SIGNATURES {
            hash_function.update(self.tail.stamps[i].signature);
            hash_function.update(self.tail.stamps[i].address);
        }
        Ok(hash_function.digest().unwrap())
    }
}

impl Block {
    /// Create a new block
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        previous_hash: StdByteArray,
        nonce: u64,
        timestamp: u64,
        transactions: Vec<Transaction>,
        miner_address: Option<StdByteArray>,
        stamps: [Stamp; N_TRANSMISSION_SIGNATURES],
        depth: u64,
        state_root: Option<StdByteArray>,
        hasher: &mut impl HashFunction,
    ) -> Self {
        let merkle_tree = generate_tree(transactions.iter().collect(), hasher).unwrap();
        let tail = BlockTail {
            stamps
        };
        let header = BlockHeader::new(
            previous_hash, 
            merkle_tree.nodes.get(merkle_tree.root.unwrap()).unwrap().hash,
            state_root, // State root is not set in this context
            nonce, 
            timestamp,
            miner_address,
            tail,
            depth,
        );
        let hash = header.hash(hasher);
        Block {
            header,
            transactions,
            hash: if let Ok(item) = hash {Some(item)} else {None},
            merkle_tree
        }
    }

    /// Creates the proof of inclusion for a transaction in the block
    pub fn get_proof_for_transaction<T: Into<StdByteArray>>(&self, transaction: T) -> Option<MerkleProof> {
        generate_proof_of_inclusion(
            &self.merkle_tree,
            transaction.into(),
            &mut DefaultHash::new()
        )
    }

    /// Veerifies a transaction is in the block
    pub fn validate_transaction<T: Into<StdByteArray> + Clone>(&self, transaction: T) -> bool{
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
        self.hash_clean(&mut DefaultHash::new()).unwrap()
    }
    
    fn sign<const K: usize, const P: usize>(&mut self, signing_function: &mut impl SigFunction<K, P, 64>) -> [u8; 64] {
        signing_function.sign(self)
    }
}

impl From<Block> for StdByteArray {
    fn from(block: Block) -> Self {
        block.hash.unwrap()
    }
}

#[cfg(test)]
mod tests {

    use pillar_crypto::signing::{DefaultSigner, SigFunction, SigVerFunction};

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
    fn test_collapse_with_move() {
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
    fn test_very_complex_collapse() {
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
        tail.collapse();
        assert_eq!(tail.n_stamps(), 5);
        assert_eq!(tail.stamps[0].signature, [1; 64]);
        assert_eq!(tail.stamps[1].signature, [2; 64]);
        assert_eq!(tail.stamps[2].signature, [3; 64]);
        assert_eq!(tail.stamps[3].signature, [4; 64]);
        assert_eq!(tail.stamps[4].signature, [5; 64]);
    }

    #[test]
    fn test_collapse_with_duplicates() {
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
            signature: [1; 64],
            address: [1; 32]
        };
        tail.collapse();
        assert_eq!(tail.n_stamps(), 2);
        assert_eq!(tail.stamps[0].address, [1; 32]);
        assert_eq!(tail.stamps[1].address, [2; 32]);
    }

    #[test]
    fn test_clean_invalid_signatures() {
        let mut tail = BlockTail::default();
        let target = BlockHeader::default(); // Assuming BlockHeader implements Default
        let private = DefaultSigner::generate_random();
        let public = private.get_verifying_function().to_bytes();
        tail.stamps[0] = Stamp {
            signature: [1; 64],
            address: public
        };
        let mut private = DefaultSigner::generate_random();
        let public = private.get_verifying_function().to_bytes();
        tail.stamps[1] = Stamp {
            signature: private.sign(&target),
            address: public
        };
        tail.clean(&target);
        assert_eq!(tail.n_stamps(), 1);
    }

    #[test]
    fn test_stamp_addition() {
        let mut tail = BlockTail::default();
        let stamp = Stamp {
            signature: [1; 64],
            address: [1; 32]
        };
        assert!(tail.stamp(stamp).is_ok());
        assert_eq!(tail.n_stamps(), 1);
        assert_eq!(tail.stamps[0].address, [1; 32]);
    }

    #[test]
    fn test_stamp_overflow() {
        let mut tail = BlockTail::default();
        for i in 0..N_TRANSMISSION_SIGNATURES {
            tail.stamps[i] = Stamp {
                signature: [(i+1) as u8; 64],
                address: [(i+1) as u8; 32]
            };
        }
        let new_stamp = Stamp {
            signature: [255; 64],
            address: [255; 32]
        };
        assert!(tail.stamp(new_stamp).is_err());
    }
}