use std::collections::HashMap;

use rand::{rng, seq::{IndexedRandom, IteratorRandom}};

use crate::{blockchain::{chain::Chain, chain_shard::ChainShard, TrimmableChain}, crypto::{hashing::{DefaultHash, HashFunction}, merkle::generate_tree}, nodes::{messages::Message, node::{Broadcaster, Node}, peer::Peer}, primitives::{block::{Block, BlockHeader, BlockTail}, transaction::Transaction}, reputation::history::NodeHistory};

use super::{peers::discover_peers, reputation::N_TRANSMISSION_SIGNATURES};

/// Queries a peer to send a block.
async fn query_block_from_peer(
    peer: &mut Peer,
    initializing_peer: &Peer,
    hash: [u8; 32]
) -> Result<Block, std::io::Error>{
    // send the block request to the peer
    // TODO better error handling
    let response = peer.communicate(&Message::BlockRequest(hash), initializing_peer).await?;
    let block = match response {
        Message::BlockResponse(block) => {
            // add the block to the chain
            match block{
                Some(block) => {
                    // we need to verify that the header validates
                    // and that the transactions are the same as declared
                    let mut result = true;
                    // check header
                    result &= block.header.validate(hash, &mut DefaultHash::new());
                    // verify merkle root
                    let tree = generate_tree(
                        block.transactions.iter().map(|x|x).collect(), 
                        &mut DefaultHash::new()
                    );
                    result = result 
                        && tree.is_ok()  // tree worked
                        && tree.as_ref().unwrap().get_root_hash().is_some() // has root
                        && tree.unwrap().get_root_hash().unwrap() == block.header.merkle_root; // root matches
                    if result{
                        Ok(block)
                    }else{
                        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Block failed verification"))
                    }
                },
                _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Peer responded wrong"))

            }
        }
        _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Peer responded wrong"))
    };
    block
}

/// Given a shard (validated) uses the node to get the chain
async fn shard_to_chain(node: &mut Node, shard: ChainShard) -> Result<Chain, std::io::Error> {
    let mut threads = Vec::new();
    // get many blocks simultaneously
    let (tx, block_channel) = flume::unbounded();
    for (hash, _) in shard.headers{
        let nodeclone = node.clone();
        let sender = tx.clone();
        let handle = tokio::spawn(async move{
            loop{ // keep asking for the node until we pass
                let mut peer = nodeclone.clone().peers.lock().await.values().choose(&mut rng()).unwrap().clone(); // random peer
                let block = query_block_from_peer(&mut peer, &nodeclone.clone().into(), hash).await;
                match block{
                    Ok(block)=>{
                        sender.send(block).unwrap();
                        break;
                    },
                    Err(_) => {}
                };
            }
        });
        threads.push(handle);
    }
    // let the threads finish
    for thread in threads {
        thread.await.unwrap();
    }
    // now all blocks should be in the channel
    // we need to work our way up by depth
    let mut blocks: Vec<Block> = block_channel.iter().collect();
    // sort by depth
    blocks.sort_by_key(|x| x.header.depth);
    // note: we know that there is exactly one genesis from shard validation
    let mut chain = Chain::new_with_genesis();
    let mut reputations = node.reputations.lock().await;
    for block in &blocks[1..]{ // skip the first - genesis
        let mut block = block.to_owned();
        let miner = block.header.miner_address.unwrap();
        let tail = block.header.tail.clone();
        let head = block.header.clone();
        let hash = block.hash.unwrap();
        loop{ // we need to keep going until it passes full validation
            match chain.add_new_block(block){
                Err(_) => { // failed validation
                    let mut peer = node.peers.lock().await.values().choose(&mut rng()).unwrap().clone(); // random peer
                    block = query_block_from_peer(&mut peer, &node.clone().into(), hash).await?; // one more attempt, then fail.
                }
                _ => {
                    // we need to update the reputations
                    settle_reputations(&mut reputations, miner, &tail, head);
                    break;
                }
            }
        }
    }
    Ok(chain)
}

/// This function takes a header and a tail of a block, as well as its miner.
/// Then, it updates a reputations map to reflect new knowledge
fn settle_reputations(reputations: &mut HashMap<[u8; 32], NodeHistory>, miner: [u8; 32], tail: &BlockTail, head: BlockHeader){
    // passed validation - we need to record reputations
    match reputations.get_mut(&miner){
        Some(history) => {
            // update the history
            history.settle_head(head);
        }
        None => {
            // create new history
            reputations.insert(miner.clone(), NodeHistory::default());
            reputations.get_mut(&miner).unwrap().settle_head(head);
        }
    }
    // now each signature gets an award
    for stamper in tail.get_stampers(){
        match reputations.get_mut(&stamper){
            Some(history) => {
                // update the history
                history.settle_tail(&tail, head);
            }
            None => {
                // create new history
                reputations.insert(miner.clone(), NodeHistory::default());
                reputations.get_mut(&miner).unwrap().settle_tail(&tail, head);
            }
        }
    }
}

/// Discovery algorithm for the chain
pub async fn dicover_chain(node: &mut Node) -> Result<Chain, std::io::Error> {
    // get the peers first
    discover_peers(node).await?;
    // broadcast the chain shard request to all peers
    let mut peers = node.peers.lock().await;
    let mut chain_shards = Vec::new();
    for (_, peer) in peers.iter_mut() {
        // send the chain shard request to the peer
        let response = peer.communicate(&Message::ChainShardRequest, &node.clone().into()).await?;
        match response {
            Message::ChainShardResponse(shard) => {
                // add the shard to the chain
                if shard.validate(){
                    chain_shards.push(shard);
                }
                // TODO perhaps blacklist the peer
            }
            _ => {}
        }
    }
    drop(peers);
    // find deepest out of peers
    let shard = deepest_shard(chain_shards)?;
    // now we have valid shards
    shard_to_chain(node, shard).await
}

/// Find the deepest chain shard - they shoudl in theory be the same but we want the longest
/// TODO: Maybe we should check agreement of hashes and such, but with POW deepest should be accurate
pub fn deepest_shard(shards: Vec<ChainShard>) -> Result<ChainShard, std::io::Error> {
    let shard = shards.iter().max_by_key(|shard| shard.leaves.iter().max_by_key(|leaf| shard.headers[*leaf].depth).unwrap());
    match shard {
        Some(shard) => Ok(shard.clone()),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No shards found",
        )),   
    }
}

/// The definition of the genisis block
pub fn get_genesis_block() -> Block{
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
        &mut DefaultHash::new()
    )
}

/// Sync the chain in a node when it comes back online
/// Avoids recomputing and entire chain when a node comes back online
/// Includes verification of new blocks and trimming of synced blocks
/// May take ownership of mutexed chain for a while
pub async fn sync_chain(node: &mut Node){
    // the sync request
    let leaves = node.chain.lock().await.as_ref().expect("No chain - discover chain instead").leaves.clone();
    let request = Message::ChainSyncRequest(leaves.clone());
    // broadcast the request
    let responses = node.broadcast(&request).await.expect("Failed to broadcast sync request");
    // sync up with the reponses
    let mut extensions: HashMap<[u8; 32], (Chain, u64)> = HashMap::new();
    for response in responses{
        match response {
            Message::ChainSyncResponse(shards) => {
                // check each shard - validate it
                for shard in shards.iter(){
                    // figure out which leaf this connect to. we can start at any arbitrary leaf because they will all end up at the same place
                    let leaf = &shard.deepest_hash;
                    let mut curr = shard.blocks.get(leaf);
                    while let Some(current_block) = curr{
                        if shard.verify_block(current_block){
                            // good, recurse
                            if leaves.contains(&current_block.header.previous_hash){
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
                            curr = shard.blocks.get(&current_block.header.previous_hash);
                        }else{
                            // TODO maybe blacklist the peer
                            break;
                        }
                    } 

                }
            }
            _ => {}
        }
    }
    // each response has been verified and we have the deepest for each leaf
    // now we need to merge the chains
    let mut chain = node.chain.lock().await.as_ref().unwrap().clone();
    for (_, (chain_extension, _)) in extensions.iter(){
        // we need to find the block in the chain
        for extension_leaf in chain_extension.leaves.iter(){ // include the forks
            let mut to_add = vec![]; // record them in order to add deepest first
            let mut curr = chain_extension.blocks.get(extension_leaf);
            // we need to travel backwards again :()
            while let Some(current_block) = curr{
                to_add.push(current_block.clone());
                // already been verified :()
                curr = chain_extension.blocks.get(&current_block.header.previous_hash);
            }
            // now we add them to the chain
            for block in to_add.iter().rev(){
                // verifies again
                chain.add_new_block(block.clone()).expect("Failed to add block");
                // and record the reputations
                let miner = block.header.miner_address.unwrap();
                let tail = block.header.tail.clone();
                let head = block.header.clone();
                let mut reputations = node.reputations.lock().await;
                settle_reputations(&mut reputations, miner, &tail, head);
            }
        }
    }
    chain.trim(); // cleanup any old forks
    // done
}