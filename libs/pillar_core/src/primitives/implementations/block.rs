use std::num::NonZeroU64;

use pillar_crypto::{hashing::{DefaultHash, HashFunction}, merkle::{generate_tree, MerkleTree}, proofs::{generate_proof_of_inclusion, verify_proof_of_inclusion, MerkleProof}, types::StdByteArray};

use crate::{primitives::{block::{Block, BlockHeader, BlockTail, Stamp}, transaction::Transaction}, protocol::reputation::N_TRANSMISSION_SIGNATURES};

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
        difficulty_target: Option<u64>,
        state_root: Option<StdByteArray>,
        hasher: &mut impl HashFunction,
    ) -> Self {
        let merkle_tree = generate_tree(transactions.iter().collect(), hasher).unwrap();
        let tail = BlockTail {
            stamps
        };
        let header = BlockHeader::new(
            None,
            previous_hash, 
            merkle_tree.nodes.get(merkle_tree.root.unwrap()).unwrap().hash,
            state_root, // State root is not set in this context
            nonce, 
            timestamp,
            miner_address,
            tail,
            depth,
            difficulty_target
        );
        Self::new_from_header_and_transactions(header, transactions)
    }

    pub fn new_from_header_and_transactions(
        header: BlockHeader, 
        transactions: Vec<Transaction>
    ) -> Self {
        Block {
            header,
            transactions
        }
    }

    pub fn get_merkle_tree(&self) -> Result<MerkleTree, std::io::Error>{
        generate_tree(self.transactions.iter().collect(), &mut DefaultHash::new())
    }

    /// Creates the proof of inclusion for a transaction in the block
    pub fn get_proof_for_transaction<T: Into<StdByteArray>>(&self, transaction: T) -> Option<MerkleProof> {
        generate_proof_of_inclusion(
            &self.get_merkle_tree().ok()?,
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