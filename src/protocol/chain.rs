use crate::{blockchain::{chain::Chain, chain_shard::ChainShard}, nodes::{messages::Message, node::{Broadcaster, Node}}};

use super::peers::discover_peers;

/// Given a shard (validated) uses te node to get the chain
pub async fn shard_to_chain(node: &mut Node, shard: ChainShard) -> Result<Chain, std::io::Error> {
    
}

/// Discovery algorithm for the chain
pub async fn dicover_chain(node: &mut Node) -> Result<(), std::io::Error> {
    // get the peers first
    discover_peers(node).await?;
    // broadcast the chain shard request to all peers
    let mut peers = node.peers.lock().await;
    let mut chain_shards = Vec::new();
    for peer in peers.iter_mut() {
        // send the chain shard request to the peer
        let response = peer.communicate(&Message::ChainShardRequest, node.clone().into()).await?;
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
    // find deepest out of peers
    let shard = deepest_shard(chain_shards)?;
    // now we have valid shards
    Ok(())
}

/// Find the deepest chain shard - they shoudl in theory be the same but we want the longest
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