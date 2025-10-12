use std::{collections::HashMap, path::PathBuf};

use pillar_crypto::types::StdByteArray;
use pillar_serialize::PillarSerialize;

use crate::{blockchain::chain::Chain, primitives::block::Block};

pub mod database;

pub(crate) trait Persistable
    where Self: Sized + PillarSerialize 
{
    async fn save(&self, path: &PathBuf) -> Result<(), std::io::Error>{
        let bytes = self.serialize_pillar()?;
        tokio::fs::write(path, bytes).await
    }
    async fn load(path: &PathBuf) -> Result<Self, std::io::Error> {
        let bytes = tokio::fs::read(path).await?;
        Self::deserialize_pillar(&bytes)
    }
}

impl Persistable for Block{} // we are using the default implementations, which is just serialize and save to a path

// chains serialization is to save one block per file
impl Persistable for Chain {
    async fn save(&self, path: &std::path::PathBuf) -> Result<(), std::io::Error> {
        if !std::fs::metadata(path)?.is_dir(){
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path must be a directory"));
        }
        for (block_hash, block) in &self.blocks {
            let block_path = path.join(format!("{}.bin", hex::encode(block_hash)));
            block.save(&block_path).await?;
        }        
        Ok(())
    }

    async fn load(path: &std::path::PathBuf) -> Result<Self, std::io::Error> {
        if !std::fs::metadata(path)?.is_dir(){
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path must be a directory"));
        }
        let block_files = std::fs::read_dir(path)?;
        let mut blocks: HashMap<StdByteArray, Block> = HashMap::new();
        for entry in block_files {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let hash: StdByteArray = hex::decode(entry.file_name().to_str().unwrap())
                    .unwrap()
                    .try_into()
                    .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid hash in filename"))?;
                let block = Block::deserialize_pillar(&tokio::fs::read(entry.path()).await?)?;
                blocks.insert(hash, block);
            }
        }
        let chain = unsafe{Chain::new_from_blocks(blocks)};
        todo!("remove unsafe call");
        Ok(chain)
    }
}