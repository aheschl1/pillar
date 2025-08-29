use std::collections::{HashMap, HashSet};

use pillar_crypto::{hashing::DefaultHash, types::StdByteArray};
use serde::{Deserialize, Serialize};

use crate::{accounting::{account::Account, state::StateManager}, primitives::{block::BlockHeader, errors::BlockValidationError}, protocol::chain::get_genesis_block};

use super::{chain::Chain, TrimmableChain};

/// chain shard is used to build up a chain given a list of block headers
/// It is responsible for the validation and construction of the chain from a new node.
/// // chain shard is just a chain of headers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChainShard {
    pub headers: HashMap<StdByteArray, BlockHeader>,
    pub leaves: HashSet<StdByteArray>
}

impl ChainShard{
    /// ensures the hashs are good, and the depths work
    pub fn validate(&self) -> Result<(), BlockValidationError>{
        let mut genesis_found = false;
        let state_manager = StateManager::new();
        let state_root = state_manager.state_trie
            .lock()
            .as_mut()
            .unwrap()
            .create_genesis([0; 32], Account::default()).unwrap();

        for (declared_hash, header) in &self.headers {
            if header.depth == 0{
                if genesis_found{
                    return Err(BlockValidationError::MalformedShard("Multiple genesis blocks found".to_string()));
                }
                // make sure valid genesis
                let correct_gensis = get_genesis_block(Some(state_root));
                if *header != correct_gensis.header{
                    return Err(BlockValidationError::MalformedShard("Genesis block does not match".to_string()));
                }
                genesis_found = true;
            }
            header.validate(
                *declared_hash,
                &mut DefaultHash::new() 
            )?;

            // check the previous hashes exists
            let previous_hash = header.previous_hash;
            let previous_block = self.headers.get(&previous_hash);

            match previous_block {
                Some(last_block) => {
                    if last_block.depth + 1 != header.depth {
                        return Err(BlockValidationError::MalformedShard("Depth does not match previous block".into()));
                    } else if header.timestamp < last_block.timestamp {
                        return Err(BlockValidationError::MalformedShard("Timestamp is before previous block".into()));
                    } else {
                        Ok(())
                    }
                },
                None => {
                    if header.depth != 0 { // genesis block has no previous hash
                        return Err(BlockValidationError::MalformedShard("Previous block not found".into()));
                    }
                    Ok(())
                }
            }?;
        }
        
        if genesis_found{
            Ok(())
        } else{
            Err(BlockValidationError::MalformedShard("No genesis block found".to_string()))
        }
    }

    pub fn get_block(&self, hash: &StdByteArray) -> Option<BlockHeader>{
        self.headers.get(hash).cloned()
    }

}

impl From<Chain> for ChainShard{
    fn from(mut chain: Chain) -> Self {
        chain.trim();
        let headers = 
            chain
            .blocks
            .iter()
            .map(|(hash, block)| (*hash, block.header)).collect::<HashMap<_, _>>();
        
        let leaves = chain.leaves;

        Self { headers, leaves }
    }
}


impl TrimmableChain for ChainShard {
    fn get_headers(&self) -> &HashMap<StdByteArray, BlockHeader> {
        &self.headers
    }

    fn get_leaves_mut(&mut self) -> &mut HashSet<StdByteArray> {
        &mut self.leaves
    }

    fn remove_header(&mut self, hash: &StdByteArray) {
        self.headers.remove(hash);
    }
}

#[cfg(test)]
mod tests {
    use pillar_crypto::signing::{DefaultSigner, SigFunction, SigVerFunction, Signable};

    use super::*;
    
    use crate::primitives::block::{Block, BlockTail};
    use crate::primitives::transaction::Transaction;
    use crate::protocol::pow::mine;

    #[tokio::test]
    async fn test_trim_removes_short_fork() {
        let mut chain = Chain::new_with_genesis();
        
        let mut signing_key = DefaultSigner::generate_random();
        // public
        let sender = signing_key.get_verifying_function().to_bytes();


        let mut parent_hash = chain.deepest_hash;
        let genesis_hash = parent_hash;
        for depth in 1..=11 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth-1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                parent_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&parent_hash).expect("Parent hash must exist");
            let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
            mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
            parent_hash = block.header.completion.unwrap().hash;
            chain.add_new_block(block).unwrap();
        }

        let long_chain_leaf = parent_hash;

        // Create shorter fork from genesis (only 1 block)
        let mut trans = Transaction::new(sender, [2; 32], 0, 0, 0, &mut DefaultHash::new());
        trans.sign(&mut signing_key);
        let mut fork_block = Block::new(
            genesis_hash, // same genesis
            0,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+30,
            vec![trans],
            None,
            BlockTail::default().stamps,
            1,
            None,
            None,
            &mut DefaultHash::new(),
        );
        let prev_header = chain.headers.get(&genesis_hash).expect("Genesis hash must exist");
        let state_root = chain.state_manager.branch_from_block_internal(&fork_block, prev_header, &sender);
        mine(&mut fork_block, sender, state_root, vec![], None, DefaultHash::new()).await;
        chain.add_new_block(fork_block.clone()).unwrap();

        assert!(chain.blocks.contains_key(&fork_block.header.completion.unwrap().hash));

        // Trim should remove the short fork
        let mut shard: ChainShard = chain.into();
        shard.trim();
        assert!(!shard.headers.contains_key(&fork_block.header.completion.unwrap().hash));
        assert!(shard.headers.contains_key(&long_chain_leaf));
        assert!(shard.headers.contains_key(&shard.headers[&long_chain_leaf].previous_hash));
    }

    #[tokio::test]
    async fn test_trim_keeps_close_forks() {
        let mut chain = Chain::new_with_genesis();
        let mut signing_key = DefaultSigner::generate_random();
        // public
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut parent_hash = chain.deepest_hash;
        let genesis_hash = parent_hash;

        // Build a main chain of depth 9
        for depth in 1..=9 {
            let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth;
            let mut transaction = Transaction::new(sender, [2; 32], 0, time, depth-1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                parent_hash,
                0,
                time,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&parent_hash).expect("Parent hash must exist");
            let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
            mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
            parent_hash = block.header.completion.unwrap().hash;
            chain.add_new_block(block).unwrap();
        }

        let mut trans = Transaction::new(sender, [2; 32], 0, 0, 0, &mut DefaultHash::new());
        trans.sign(&mut signing_key);
        // Add a 1-block fork off the genesis (difference = 9)
        let mut fork_block = Block::new(
            genesis_hash,
            0,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 50,
            vec![trans],
            None,
            BlockTail::default().stamps,
            1,
            None,
            None,
            &mut DefaultHash::new(),
        );
        let prev_header = chain.headers.get(&genesis_hash).expect("Genesis hash must exist");
        let state_root = chain.state_manager.branch_from_block_internal(&fork_block, prev_header, &sender);
        mine(&mut fork_block, sender, state_root, vec![], None, DefaultHash::new()).await;
        let fork_hash = fork_block.header.completion.unwrap().hash;
        chain.add_new_block(fork_block).unwrap();

        // This fork is <10 behind, so it should NOT be trimmed
        let mut shard: ChainShard = chain.into();
        shard.trim();
        assert!(shard.headers.contains_key(&fork_hash));
    }

    #[tokio::test]
    async fn test_trim_multiple_short_forks() {
        let mut chain = Chain::new_with_genesis();
        let mut signing_key = DefaultSigner::generate_random();
        // public
        let sender = signing_key.get_verifying_function().to_bytes();

        let mut main_hash = chain.deepest_hash;
        let genesis_hash = main_hash;

        // Extend the main chain to depth 12
        for depth in 1..=12 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, depth-1, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut block = Block::new(
                main_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + depth,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                depth,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&main_hash).expect("Parent hash must exist");
            let state_root = chain.state_manager.branch_from_block_internal(&block, prev_header, &sender);
            mine(&mut block, sender, state_root, vec![], None, DefaultHash::new()).await;
            main_hash = block.header.completion.unwrap().hash;
            chain.add_new_block(block).unwrap();
        }

        // Create two short forks from genesis (depth 1)
        let mut fork_hashes = vec![];
        for _ in 0..2 {
            let mut transaction = Transaction::new(sender, [2; 32], 0, 0, 0, &mut DefaultHash::new());
            transaction.sign(&mut signing_key);
            let mut fork_block = Block::new(
                genesis_hash,
                0,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 20,
                vec![transaction],
                None,
                BlockTail::default().stamps,
                1,
                None,
                None,
                &mut DefaultHash::new(),
            );
            let prev_header = chain.headers.get(&genesis_hash).expect("Genesis hash must exist");
            let state_root = chain.state_manager.branch_from_block_internal(&fork_block, prev_header, &sender);
            mine(&mut fork_block, sender, state_root, vec![], None, DefaultHash::new()).await;
            let hash = fork_block.header.completion.unwrap().hash;
            fork_hashes.push(hash);
            chain.add_new_block(fork_block).unwrap();
        }

        // Verify they were added
        for hash in &fork_hashes {
            assert!(chain.blocks.contains_key(hash));
            assert!(chain.leaves.contains(hash));
        }

        // Run trim — both forks should be removed
        let mut shard: ChainShard = chain.into();
        shard.trim();

        for hash in &fork_hashes {
            assert!(!shard.headers.contains_key(hash));
            assert!(!shard.leaves.contains(hash));
        }

        assert_eq!(shard.leaves.len(), 1); // only the longest chain remains
        assert!(shard.headers.contains_key(&main_hash));
        // check that the right number of blocks remain
        assert_eq!(shard.headers.len(), 13); // 12 from main chain + genesis
        // actually count the number of blocks in the main chain
        let mut current_hash = main_hash;
        let mut count = 0;
        while let Some(block) = shard.headers.get(&current_hash) {
            count += 1;
            current_hash = block.previous_hash;
        }
        assert_eq!(count, 13); // 12 blocks in the main chain
    }

}