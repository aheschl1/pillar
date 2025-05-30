use crate::{blockchain::chain::Chain, primitives::block::Block};

pub trait Datastore: Send + Sync {
    /// If a chain exists on disk.
    /// 
    /// Returns `Some(chain_timestamp)` if the chain exists, or `None` if it does not.
    fn latest_chain(&self) -> Option<u64>;

    /// Loads chain from disk.
    /// 
    /// Returns a `Chain` if it exists, or an error if it does not.
    fn load_chain(&self) -> Result<Chain, std::io::Error>;

    /// Saves a chain to disk.
    fn save_chain(&self, chain: Chain) -> Result<(), std::io::Error>;

    /// Saves a block to disk.
    /// 
    /// Returns `Ok(())` if the block was saved successfully, or an error if it was not.
    fn save_block(&self, block: Block) -> Result<(), std::io::Error>;

    /// Loads a block from disk.
    fn load_block(&self, block_hash: &str) -> Result<Block, std::io::Error>;

}

pub struct GenesisDatastore;

impl Datastore for GenesisDatastore {
    fn latest_chain(&self) -> Option<u64> {
        None
    }

    fn load_chain(&self) -> Result<Chain, std::io::Error> {
        Ok(Chain::new_with_genesis())
    }

    fn save_chain(&self, _chain: Chain) -> Result<(), std::io::Error> {
        Ok(())
    }

    fn save_block(&self, _block: Block) -> Result<(), std::io::Error> {
        Ok(())
    }

    fn load_block(&self, _block_hash: &str) -> Result<Block, std::io::Error> {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Genesis block does not exist",
        ))
    }
}