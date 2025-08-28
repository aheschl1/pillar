use std::{collections::{HashMap, HashSet}};

use pillar_crypto::{hashing::{DefaultHash, Hashable}, merkle::generate_tree, types::StdByteArray};
use rand::{rng, seq::{IteratorRandom}};
use tracing::{instrument, warn};

use crate::{blockchain::{chain::Chain, chain_shard::ChainShard, TrimmableChain}, nodes::{node::{Broadcaster, Node, NodeState}, peer::Peer}, primitives::{block::{Block, BlockTail}, errors::{BlockValidationError, QueryError}, messages::Message, transaction::Transaction}, protocol::difficulty::MIN_DIFFICULTY};

use super::peers::discover_peers;

/// Queries a peer to send a block.
async fn query_block_from_peer(
    peer: &mut Peer,
    initializing_peer: &Peer,
    hash: StdByteArray
) -> Result<Block, QueryError>{
    // send the block request to the peer
    // TODO better error handling
    let response = peer.communicate(&Message::BlockRequest(hash), initializing_peer).await.map_err(
        QueryError::IOError
    )?;
    let block = match response {
        Message::BlockResponse(Some(block)) => {
            // we need to verify that the header validates
            // and that the transactions are the same as declared
            block.header.validate(hash, &mut DefaultHash::new()).map_err(
                QueryError::BadBlock
            )?;
            // verify merkle root
            let tree = generate_tree(
                block.transactions.iter().collect(), 
                &mut DefaultHash::new()
            );

            if tree.is_err() || tree.as_ref().unwrap().get_root_hash().is_none(){
                warn!("Merkle tree generation failed: {:?}", tree.err());
                return Err(QueryError::BadBlock(BlockValidationError::MalformedBlock("Merkle tree generation failed".to_string())));
            }
            if block.header.merkle_root != tree.unwrap().get_root_hash().unwrap() {
                return Err(QueryError::BadBlock(BlockValidationError::MalformedBlock("Merkle root does not match".to_string())));
            }
            Ok(block)
        }
        _ => Err(QueryError::InvalidResponse)
    }?;
    Ok(block)
}

/// Given a shard (validated) uses the node to get the chain
async fn shard_to_chain(node: &mut Node, shard: ChainShard) -> Result<Chain, QueryError> {
    let mut threads = Vec::new();
    // get many blocks simultaneously
    for (hash, _) in shard.headers{
        let nodeclone = node.clone();
        let handle = tokio::spawn(async move{
            loop{ // keep asking for the node until we pass
                let mut peer = nodeclone.clone().inner.peers.read().await.values().choose(&mut rng()).unwrap().clone(); // random peer
                let block = query_block_from_peer(&mut peer, &nodeclone.clone().into(), hash).await;
                if let Ok(block) = block {
                    // we got the block, send it to the channel
                    return block;
                }
            }
        });
        threads.push(handle);
    }
    // let the threads finish
    let mut blocks: Vec<Block> = Vec::new();
    for thread in threads {
        blocks.push(thread.await.unwrap());
    }
    // we need to work our way up by depth
    // sort by depth
    blocks.sort_by_key(|x| x.header.depth);
    // note: we know that there is exactly one genesis from shard validation
    let mut chain = Chain::new_with_genesis();
    for block in &blocks[1..]{ // skip the first - genesis
        let mut block = block.to_owned();
        let hash = block.hash.unwrap();
        loop{ // we need to keep going until it passes full validation
            match chain.add_new_block(block){
                Err(_) => { // failed validation
                    let mut peer = node.inner.peers.read().await.values().choose(&mut rng()).unwrap().clone(); // random peer
                    block = query_block_from_peer(&mut peer, &node.clone().into(), hash).await?; // one more attempt, then fail.
                }
                _ => {
                    break;
                }
            }
        }
    }
    Ok(chain)
}


/// Discovery algorithm for the chain
pub async fn dicover_chain(mut node: Node) -> Result<(), QueryError> {
    // get the peers first
    discover_peers(&mut node).await.map_err(
        QueryError::IOError
    )?;
    // broadcast the chain shard request to all peers
    let peers = node.inner.peers.read().await;
    let mut chain_shards = Vec::new();
    for (_, peer) in peers.iter() {
        // send the chain shard request to the peer
        let response = peer.communicate(&Message::ChainShardRequest, &(node.clone().into())).await
            .map_err(QueryError::IOError)?;
        if let Message::ChainShardResponse(shard) = response {
            // add the shard to the chain   
            shard.validate().map_err(
                QueryError::BadBlock
            )?;
            chain_shards.push(shard);
            // TODO perhaps blacklist the peer
        }  

    }
    drop(peers);
    // find deepest out of peers
    let shard = deepest_shard(&chain_shards)?;
    // now we have valid shards
    let chain = shard_to_chain(&mut node, shard.clone()).await?;
    node.inner.chain.lock().await.replace(chain);
    Ok(())
}

/// Find the deepest chain shard - they shoudl in theory be the same but we want the longest
/// TODO: Maybe we should check agreement of hashes and such, but with POW deepest should be accurate
pub fn deepest_shard(shards: &[ChainShard]) -> Result<ChainShard, QueryError> {
    let shard = shards.iter().max_by_key(|shard| shard.leaves.iter().max_by_key(|leaf| shard.headers[*leaf].depth).unwrap());
    match shard {
        Some(shard) => Ok(shard.clone()),
        None => Err(QueryError::NoReply),   
    }
}

/// The definition of the genisis block
pub fn get_genesis_block(state_root: Option<StdByteArray>) -> Block{
    Block::new(
        [0; 32], 
        0, 
        0, 
        vec![
            Transaction::new([0; 32], [0;32], 0, 0, 0, &mut DefaultHash::new())
        ], 
        Some([0; 32]),
        BlockTail::default().stamps,
        0,
        Some(MIN_DIFFICULTY), // difficulty target
        state_root,
        &mut DefaultHash::new()
    )
}

/// Sync the chain in a node when it comes back online
/// Avoids recomputing and entire chain when a node comes back online
/// Includes verification of new blocks and trimming of synced blocks
/// May take ownership of mutexed chain for a while
/// TODO this may try to duplicate if there are forks in extensions - fix the final portion of the sync
#[instrument(fields(node = ?node.inner.public_key))]
pub async fn sync_chain(node: Node) -> Result<(), QueryError> {
    if node.inner.peers.read().await.is_empty(){
        tracing::info!("No peers to sync with, skipping chain sync");
        return Ok(());
    }
    // the sync request
    let mut chain = node.inner.chain.lock().await;
    if chain.is_none() {
        return Err(QueryError::InsufficientInfo("Chain is not initialized".to_string()));
    }
    let chain = chain.as_mut().unwrap();
    let leaves = chain.leaves.clone();
    
    let request = Message::ChainSyncRequest(leaves.iter().copied().collect());
    // broadcast the request
    let responses = node.broadcast(&request).await.map_err(
        QueryError::IOError
    )?;
    if responses.is_empty() {
        tracing::info!("No responses to chain sync request, skipping sync");
        return Ok(());
    }
    tracing::debug!("Received {} responses to chain sync request", responses.len());

    // sync up with the reponses
    let mut extensions: HashMap<StdByteArray, (Chain, u64)> = HashMap::new();
    for response in responses{
        match response {
            Message::ChainSyncResponse(mut shards) => {
                // check each shard - validate it
                for shard in shards.iter_mut(){
                    // figure out which leaf this connect to. we can start at any arbitrary leaf because they will all end up at the same place
                    let leaf = &shard.deepest_hash;
                    let mut curr = shard.blocks.get(leaf).cloned();
                    tracing::debug!("Length of shard: {}", shard.blocks.len());
                    while let Some(current_block) = curr{
                        // two things can happen here - if the existing chain has the previous block
                        // then we need to validate the block on that chain - otherwise validate on the shard
                        tracing::debug!("Found connection {:?}", leaves.contains(&current_block.header.previous_hash));
                        if leaves.contains(&current_block.header.previous_hash) && chain.verify_block(&current_block).is_ok(){ // double check that this is a valid connection by verifying the block
                            // we have reached it. record this verified shard
                            if let Some((_, existing_depth)) = extensions.get(&current_block.header.previous_hash) {
                                // insert if this is deeper
                                if shard.depth > *existing_depth {
                                    extensions.insert(current_block.header.previous_hash, (shard.clone(), shard.depth));
                                }
                            } else {
                                // insert if this is the first
                                extensions.insert(current_block.header.previous_hash, (shard.clone(), shard.depth));
                            }
                            break;
                        }
                        curr = shard.blocks.get(&current_block.header.previous_hash).cloned();
                    } 

                }
            }
            _ => {
                tracing::debug!("Received unexpected message during chain sync: {:?}", response);
            }
        }
    }
    tracing::debug!("Found {} extensions - the first is of length {}", extensions.len(), if extensions.is_empty() { 0 } else { extensions.values().next().unwrap().0.blocks.len() });
    // each response has been verified and we have the deepest for each leaf
    // now we need to merge the chains
    for (_, (chain_extension, _)) in extensions.iter(){
        // we need to find the block in the chain
        for extension_leaf in chain_extension.leaves.iter(){ // include the forks
            let mut to_add = vec![]; // record them in order to add shallowest first
            let mut curr = chain_extension.blocks.get(extension_leaf);
            // we need to travel backwards again :()
            while let Some(current_block) = curr{
                to_add.push(current_block.clone());
                // already been verified :()
                curr = chain_extension.blocks.get(&current_block.header.previous_hash);
            }
            // now we add them to the chain
            tracing::debug!("Adding {} blocks to the chain", to_add.len());
            for block in to_add.iter().rev(){
                // verifies again
                chain.add_new_block(block.clone()).map_err(
                    QueryError::BadBlock
                )?;
            }
        }
    }
    tracing::info!("Sync complete, chain length is now {}", chain.blocks.len());
    chain.trim(); // cleanup any old forks
    tracing::debug!("Chain trimmed, length is now {}", chain.blocks.len());
    // done
    Ok(())
}


/// given a set of leaves, we need to provide chains that come after them: i.e. "missing chains"
pub async fn service_sync(node: Node, leaves: &Vec<StdByteArray>) -> Result<Vec<Chain>, std::io::Error> {
    let my_leaves = node.inner.chain.lock().await.as_ref().unwrap().leaves.clone();
    let missing_leaves = my_leaves.iter().filter(|x| !leaves.contains(*x)).cloned().collect::<Vec<_>>();
    // for each of these, we need to work our way backwards from the nodes chain off the leaf
    // we recurse until we find a node that is in `leaves` - end the chain there.
    let chain = node.inner.chain.lock().await.as_ref().unwrap().clone();
    let chains: Vec<Chain> = missing_leaves.iter().map(|leaf|{
        let mut curr = chain.blocks.get(leaf);
        let mut blocks = HashMap::new();
        while let Some(current_block) = curr {
            blocks.insert(current_block.header.hash(&mut DefaultHash::new()).unwrap(), current_block.clone());
            if leaves.contains(&current_block.header.previous_hash) {
                // we have reached the end of the chain
                break;
            }
            curr = chain.blocks.get(&current_block.header.previous_hash);
        }
        Chain::new_from_blocks(blocks)
    }).collect();

    Ok(chains)
}


/// Consumer for the block settle queue
/// Waits until the node is_serve and then starts to consume blocks in the queue
/// adding them to the chain
#[instrument(fields(node = ?node.inner.public_key), skip_all)]
pub async fn block_settle_consumer(node: Node, stop_signal: Option<flume::Receiver<()>>){
    loop{
        if let Some(signal) = &stop_signal {
            if signal.try_recv().is_ok() {break;}
        }
        let state = node.inner.state.read().await.clone();
        if !state.is_consume() {continue;}
        if let Some(block) = node.inner.late_settle_queue.dequeue(){
            tracing::debug!("Block poped from settle queue: {:?}", block.hash);
            let mut chain_lock = node.inner.chain.lock().await;
            let mut chain = chain_lock.as_mut().unwrap();
            if chain.get_block(&block.hash.unwrap()).is_some(){
                warn!("Block already exists in chain, skipping settlement: {:?}", block.hash.unwrap());
                continue; // block already exists, skip
            }
            if chain.get_block(&block.header.previous_hash).is_none() {
                warn!("Block previous hash not found in chain, triggering chain sync mode");
                drop(chain_lock); // free the lock before we call sync
                let initial_state = node.inner.state.read().await.clone();
                *node.inner.state.write().await = NodeState::ChainSyncing;
                let result = sync_chain(node.clone()).await.map_err(|e| {
                    warn!("Failed to sync chain: {:?}. Skipping block settlement.", e);
                    e
                });
                *node.inner.state.write().await = initial_state; // restore state
                if result.is_err() {continue;} // failed to sync chain, skip settlement

                chain_lock = node.inner.chain.lock().await;
                chain = chain_lock.as_mut().unwrap();
                
                if chain.get_block(&block.header.previous_hash).is_none(){
                    warn!("Block previous hash still not found in chain after sync, skipping settlement: {:?}", block.header.previous_hash);
                    continue; // still not found, skip
                }

            }
            // then we settle the block
            tracing::info!("Settling mined block with miner address: {:?}", block.header.completion.as_ref().map(|c| c.miner_address));
            if chain.add_new_block(block.clone()).is_err() {continue;} // failed to add the block
            tracing::info!("Valid block added to chain.");
            drop(chain_lock); // free lock cause why not
            if let Some(ref pool) = node.miner_pool{
                // signal to stop trying to mine the current block
                let _ = pool.mine_abort_sender.send(block.header.depth);
            }
            // the stamping process is done. do, if there is a miner address, then stamping is done on this block.
            tracing::info!("Reputations recorded for new block.");
        }
    }
}


// tests
mod tests {
    use pillar_crypto::hashing::{DefaultHash, Hashable};

    use crate::protocol::{chain::get_genesis_block, difficulty::MIN_DIFFICULTY, pow::{get_difficulty_for_block, is_valid_hash}};

    #[test]
    fn test_genesis_block(){
        let block = get_genesis_block(Some([0u8; 32]));
        let hash = Hashable::hash(&block.header, &mut DefaultHash::new()).unwrap();
        assert!(get_difficulty_for_block(&block.header, &vec![]).0 == MIN_DIFFICULTY.get());
        assert!(is_valid_hash(MIN_DIFFICULTY.get(), &hash));
    }
}