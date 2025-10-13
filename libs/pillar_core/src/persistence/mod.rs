use std::{collections::{HashMap, HashSet}, path::PathBuf};

use bytemuck::{Pod, Zeroable};
use pillar_crypto::{hashing::Hashable, types::StdByteArray};
use pillar_serialize::{PillarFixedSize, PillarNativeEndian, PillarSerialize};

use crate::{accounting::state::StateManager, blockchain::chain::Chain, primitives::block::{Block, BlockHeader}};

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

impl<V> Persistable for Vec<V> where V: PillarSerialize {}

impl Persistable for Block{} // we are using the default implementations, which is just serialize and save to a path
impl Persistable for StateManager {}

#[inline]
fn load_blocks_from_dir(path: &PathBuf) -> Result<HashMap<StdByteArray, Block>, std::io::Error>{
    let block_files = std::fs::read_dir(path)?;
    let mut blocks: HashMap<StdByteArray, Block> = HashMap::new();
    for entry in block_files {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let hash: StdByteArray = hex::decode(entry.file_name().to_str().unwrap())
                .unwrap()
                .try_into()
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid hash in filename"))?;
            let block = Block::deserialize_pillar(&std::fs::read(entry.path())?)?;
            blocks.insert(hash, block);
        }
    }
    Ok(blocks)
}

#[inline]
fn get_headers_from_blocks(blocks: &HashMap<StdByteArray, Block>) -> HashMap<StdByteArray, BlockHeader> {
    let mut headers = HashMap::new();
    for (hash, block) in blocks {
        headers.insert(*hash, block.header.clone());
    }
    headers
}

#[derive(Pod, Zeroable, Copy, Clone)]
#[repr(C)]
struct ChainMeta {
    deepest_hash: StdByteArray,
    depth: u64,
    _padding: [u8; 24], // padding to make the struct size a multiple of 32 bytes
}

impl PillarNativeEndian for ChainMeta {
    fn to_le(&mut self) {
        self.depth = self.depth.to_le();
    }
}
impl PillarFixedSize for ChainMeta {}
impl Persistable for ChainMeta {}

// chains serialization is to save one block per file
impl Persistable for Chain {
    async fn save(&self, path: &std::path::PathBuf) -> Result<(), std::io::Error> {
        if !std::fs::metadata(path)?.is_dir(){
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path must be a directory"));
        }
        for (block_hash, block) in &self.blocks {
            let block_path = path.join(format!("{}.bin", hex::encode(block_hash)));
            block.save(&block_path).await?;
        };
        let metadata = ChainMeta {
            deepest_hash: self.deepest_hash,
            depth: self.depth,
            _padding: [0u8; 24],
        };
        let meta_path = path.join("meta.bin");
        let state_manager_path = path.join("state.bin");
        let leaves_path = path.join("leaves.bin");
        metadata.save(&meta_path).await?;
        // TODO this clone sus af (implment serialization for reference types)
        self.leaves.iter().cloned().collect::<Vec<_>>().save(&leaves_path).await?;
        self.state_manager.save(&state_manager_path).await?;
        Ok(())
    }

    async fn load(path: &std::path::PathBuf) -> Result<Self, std::io::Error> {
        if !std::fs::metadata(path)?.is_dir(){
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path must be a directory"));
        }
        let blocks = load_blocks_from_dir(path)?;
        let headers = get_headers_from_blocks(&blocks);
        let metadata = ChainMeta::load(&path.join("meta.bin")).await?;
        let state_manager = StateManager::load(&path.join("state.bin")).await?;
        let leaves = Vec::<StdByteArray>::load(&path.join("leaves.bin")).await?
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

        let chain = Chain {
            deepest_hash: metadata.deepest_hash,
            depth: metadata.depth,
            blocks,
            headers,
            leaves,
            state_manager,
        };
        Ok(chain)
    }
}