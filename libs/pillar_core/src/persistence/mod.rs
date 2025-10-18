use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use bytemuck::{Pod, Zeroable};
use pillar_crypto::types::StdByteArray;
use pillar_serialize::{PillarFixedSize, PillarNativeEndian, PillarSerialize};

use crate::{
    accounting::{state::StateManager, wallet::Wallet}, blockchain::chain::Chain, nodes::peer::PillarIPAddr, primitives::block::{Block, BlockHeader}
};
pub mod manager;

pub(crate) trait Persistable
where
    Self: PillarSerialize,
{
    async fn save(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        let bytes = self.serialize_pillar()?;
        tokio::fs::write(path, bytes).await
    }
    async fn load(path: &PathBuf) -> Result<Self, std::io::Error> {
        let bytes = tokio::fs::read(path).await?;
        Self::deserialize_pillar(&bytes)
    }
}

impl<V> Persistable for Vec<V> where V: PillarSerialize {}
impl<K, V> Persistable for HashMap<K, V>
where
    K: PillarSerialize + std::hash::Hash + Eq,
    V: PillarSerialize,
{}

impl Persistable for String {}

impl Persistable for Block {} // we are using the default implementations, which is just serialize and save to a path
impl Persistable for StateManager {}

#[inline]
fn load_blocks_from_dir(path: &PathBuf) -> Result<HashMap<StdByteArray, Block>, std::io::Error> {
    let block_files = std::fs::read_dir(path)?;
    let mut blocks: HashMap<StdByteArray, Block> = HashMap::new();
    for entry in block_files {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.file_name().len() == 68 {
            let hash: StdByteArray = hex::decode(entry.file_name().to_string_lossy().split(".").next().unwrap())
                .unwrap()
                .try_into()
                .map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid hash in filename")
                })?;
            let block = Block::deserialize_pillar(&std::fs::read(entry.path())?)?;
            blocks.insert(hash, block);
        }
    }
    Ok(blocks)
}

#[inline]
fn get_headers_from_blocks(
    blocks: &HashMap<StdByteArray, Block>,
) -> HashMap<StdByteArray, BlockHeader> {
    let mut headers = HashMap::new();
    for (hash, block) in blocks {
        headers.insert(*hash, block.header);
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
impl Persistable for StdByteArray {}
impl Persistable for PillarIPAddr {}
impl Persistable for u16 {}
impl Persistable for Wallet {}

// chains serialization is to save one block per file
impl Persistable for Chain {
    async fn save(&self, path: &std::path::PathBuf) -> Result<(), std::io::Error> {
        if !std::fs::metadata(path)?.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Path must be a directory",
            ));
        }
        for (block_hash, block) in &self.blocks {
            // println!("{}", hex::encode(block_hash));
            let block_path = path.join(format!("{}.bin", hex::encode(block_hash)));
            block.save(&block_path).await?;
        }
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
        self.leaves
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .save(&leaves_path)
            .await?;
        self.state_manager.save(&state_manager_path).await?;
        Ok(())
    }

    async fn load(path: &std::path::PathBuf) -> Result<Self, std::io::Error> {
        if !std::fs::metadata(path)?.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Path must be a directory",
            ));
        }
        let blocks = load_blocks_from_dir(path)?;
        let headers = get_headers_from_blocks(&blocks);
        let metadata = ChainMeta::load(&path.join("meta.bin")).await?;
        let state_manager = StateManager::load(&path.join("state.bin")).await?;
        let leaves = Vec::<StdByteArray>::load(&path.join("leaves.bin"))
            .await?
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pillar_crypto::hashing::DefaultHash;
    use pillar_crypto::signing::{DefaultSigner, SigFunction, SigVerFunction, Signable};

    use crate::blockchain::chain::Chain;
    use crate::persistence::Persistable;
    use crate::primitives::block::{Block, BlockTail};
    use crate::primitives::transaction::Transaction;
    use crate::protocol::pow::mine;

    #[tokio::test]
    async fn test_persist_block() {
        let t = Transaction::new([1; 32], [2; 32], 0, 0, 0, &mut DefaultHash::new());
        let block = Block::new(
            [0; 32],
            0,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            vec![t],
            None,
            BlockTail::default().stamps,
            0,
            None,
            None,
            &mut DefaultHash::new(),
        );
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = PathBuf::from(file.path());
        block.save(&path).await.unwrap();

        let loaded_block = Block::load(&path).await.unwrap();
        assert_eq!(block, loaded_block);
    }

    #[tokio::test]
    async fn test_chain_persistence() {
        let mut chain = Chain::new_with_genesis();
        let mut signing_key = DefaultSigner::generate_random();
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut main_hash = chain.deepest_hash;
        let genesis_hash = main_hash;

        // Extend the main chain to depth 10
        for depth in 1..=15 {
            let mut transaction =
                Transaction::new(sender, [2; 32], 0, 0, depth - 1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                main_hash,
                0,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + depth,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain
                .headers
                .get(&block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root =
                chain
                    .state_manager
                    .branch_from_block_internal(&block, prev_header, &sender);
            mine(
                &mut block,
                sender,
                state_root,
                vec![],
                None,
                DefaultHash::new(),
            )
            .await;
            main_hash = block.header.completion.as_ref().unwrap().hash;
            chain.add_new_block(block).unwrap();
        }

        // Create a fork that shares some nodes with the main chain
        let mut fork_hash = genesis_hash;
        for depth in 1..=3 {
            let mut transaction =
                Transaction::new(sender, [2; 32], 0, 0, depth - 1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut fork_block = Block::new(
                fork_hash,
                0,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + depth
                    + 15,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain
                .headers
                .get(&fork_block.header.previous_hash)
                .expect("Previous block header not found");
            let state_root =
                chain
                    .state_manager
                    .branch_from_block_internal(&fork_block, prev_header, &sender);
            mine(
                &mut fork_block,
                sender,
                state_root,
                vec![],
                None,
                DefaultHash::new(),
            )
            .await;
            fork_hash = fork_block.header.completion.as_ref().unwrap().hash;
            chain.add_new_block(fork_block).unwrap();
        }

        // write the chain, then get a new one

        let temp_dir = tempfile::tempdir().unwrap();
        let path = PathBuf::from(temp_dir.path());
        chain.save(&path).await.unwrap();

        let new_chain = Chain::load(&path).await.unwrap();

        assert_eq!(new_chain.depth, chain.depth);
        assert_eq!(new_chain.leaves, chain.leaves);
        assert_eq!(new_chain.blocks.len(), chain.blocks.len());
        assert_eq!(new_chain.headers.len(), chain.headers.len());
        assert_eq!(new_chain.deepest_hash, chain.deepest_hash);
    }
}
