use std::collections::HashMap;

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

    /// sync the on disk state with a new chain
    /// This will write/remove as needed to ensure the on disk state matches the chain.
    fn sync_chain(&self, chain: Chain) -> Result<(), std::io::Error>;

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
        unimplemented!()
    }

    fn sync_chain(&self, _chain: Chain) -> Result<(), std::io::Error> {
        unimplemented!()
    }
}

pub struct SledDatastore {
    data: sled::Db,
}

impl SledDatastore {
    pub fn new(address: String) -> Self {
        SledDatastore { 
            data: sled::open(address).expect("Failed to open sled database"),
        }
    }
}

impl Datastore for SledDatastore {
    fn latest_chain(&self) -> Option<u64> {
        let timestamp = self.data.get("latest_timestamp");
        match timestamp {
            Ok(Some(value)) => {
                let timestamp = u64::from_le_bytes(
                    value.as_ref().try_into().expect("Failed to convert timestamp to u64")
                );
                timestamp.into()
            },
            _ => None,
        }
    }

    fn load_chain(&self) -> Result<Chain, std::io::Error> {
        let leaf_hashes = self.data.get("leaf_hashes")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        // the leaf hash's will point off to the respective blocks. then, we will work our way backwards, loading each block.
        // from there, we reconstruct the chain
        let mut blocks: HashMap<[u8; 32], Block>;
        
    }

    fn save_chain(&self, chain: Chain) -> Result<(), std::io::Error> {
        todo!()
    }

    fn save_block(&self, block: Block) -> Result<(), std::io::Error> {
        todo!()
    }

    fn load_block(&self, block_hash: &str) -> Result<Block, std::io::Error> {
        todo!()
    }

    fn sync_chain(&self, chain: Chain) -> Result<(), std::io::Error> {
        todo!()
    }
}