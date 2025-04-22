use crate::{crypto::hashing::{HashFunction, Sha3_256Hash}, primitives::block::Block};


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

    pub fn verify_block(&self, block: &Block) -> bool {
        
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