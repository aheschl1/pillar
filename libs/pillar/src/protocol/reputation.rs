
use core::f64;
use std::collections::HashSet;

use pillar_crypto::types::StdByteArray;

use crate::{nodes::{messages::Message, node::{Broadcaster, Node, ReputationMap}, peer::Peer}, primitives::block::BlockHeader, reputation::history::NodeHistory};

const MINING_WORTH_HALF_LIFE: f64 = 8f64;
const MINING_WORTH_MAX: f64 = 1f64;
// scale the worth of transmitting a block vs mining it
pub const BLOCK_STAMP_SCALING: f64 = 0.01f64;
pub const N_TRANSMISSION_SIGNATURES: usize = 10;

/// this function will scale the worth of a block over time
/// the worth of a block is 1 at the time of mining
/// Based on a half life centered around the current time
/// 0.5^((current_time - block_time) / MINING_WORTH_HALF_LIFE)
/// Beware of floating point errors
/// 
/// # Arguments
/// * `block_time` - the time of mining in seconds since epoch
pub fn block_worth_scaling_fn(block_time: u64) -> f64 {
    // days since epoch
    let current_time = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as f64) / 60.0 / 60.0 / 24.0;
    
    let block_time = block_time as f64 / 60.0 / 60.0 / 24.0;

    let diff = block_time - current_time;
    let worth = (0.5f64).powf(-diff / MINING_WORTH_HALF_LIFE);
    f64::min(worth, MINING_WORTH_MAX)
}

/// Given a reputation map, return which peers are between the lower_n-th and upper_n-th percentile
/// ex: lower_n = 0.1, upper_n = 0.9 return all peers that are at least 10th percentile, but no better than 90th percentile
/// Recommended usage is to serve addreses to external clients.
/// 
/// # Arguments
/// * `n` - the percentile to compute
/// * `reputations` - the reputation map
/// 
/// # Returns
/// * A vector of peers that are in the n-th percentile
pub fn nth_percentile_peer(lower_n: f32, upper_n: f32, reputations: &ReputationMap) -> Vec<StdByteArray>{
    if !(0.0..=1.0).contains(&lower_n)  || !(0.0..=1.0).contains(&upper_n) {
        panic!("n must be between 0 and 1");
    }
    let mut reputations = reputations.iter().map(|(a, h)|{(a, h.compute_reputation())}).collect::<Vec<_>>();
    reputations.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap()); // this is ordering in ascending order
    let lower_n = (lower_n * reputations.len() as f32).round() as usize;
    let upper_n = (upper_n * reputations.len() as f32).round() as usize;
    let mut peers = Vec::new();
    for i in reputations.iter().take(upper_n).skip(lower_n) {
        peers.push(*i.0);
    }
    peers
}

/// Given a node, query all peers for their reputations
/// This will return a vector of peers that are between the lower_n-th and upper_n-th percentile
/// Only takes the inetrsection of all peer responses
pub async fn query_for_peers_by_reputation(node: Node, lower_n: f32, upper_n: f32) -> Result<Vec<Peer>, std::io::Error>{
    let request = Message::PercentileFilteredPeerRequest(lower_n, upper_n);
    let responses = node.broadcast(&request).await?;
    // get the peer PROPOSALS
    let responses: Vec<HashSet<&Peer>> = responses.iter().map(|m|{
        match m{
            Message::PercentileFilteredPeerResponse(peers) => {
                peers.iter().collect::<HashSet<_>>()
            },
            _ => {
                panic!("Expected PercentileFilteredPeerResponse, got {:?}", m); // TODO dont just panic here
            }
        }
    }).collect();
    // the final response is the intersection of all responses
    let mut final_response = responses[0].clone();
    for i in responses.iter().skip(1) {
        final_response = final_response.intersection(i).cloned().collect();
    }
    Ok(final_response.iter().cloned().cloned().collect::<Vec<Peer>>()) // super sus souble clone here
}

/// This function takes a header and a tail of a block, as well as its miner.
/// Then, it updates a reputations map to reflect new knowledge
pub fn settle_reputations(reputations: &mut ReputationMap, head: BlockHeader){
    let miner = head.miner_address
        .expect("Expect a miner for settling reputations");

    // passed validation - we need to record reputations
    match reputations.get_mut(&miner){
        Some(history) => {
            // update the history
            history.settle_head(head);
        }
        None => {
            // create new history
            reputations.insert(miner, NodeHistory::new(miner, vec![], vec![], 0));
            reputations.get_mut(&miner).unwrap().settle_head(head);
        }
    }
    // now each signature gets an award
    for stamper in head.tail.iter_stamps(){
        match reputations.get_mut(&stamper.address){
            Some(history) => {
                // update the history
                history.settle_tail(&head.tail, head);
            }
            None => {
                // create new history
                reputations.insert(stamper.address, NodeHistory::new(stamper.address, vec![], vec![], 0));
                let history = reputations.get_mut(&stamper.address).unwrap();
                history.settle_tail(&head.tail, head);
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_worth_scaling_fn_now() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let worth = block_worth_scaling_fn(block_time);
        assert_eq!(worth, 1.0);
    }

    #[test]
    fn test_block_worth_scaling_fn_halflife_days_ago() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 - (MINING_WORTH_HALF_LIFE * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64);
        assert_eq!(worth, MINING_WORTH_MAX / 2.0);
    }

    #[test]
    fn test_block_worth_scaling_fn_halflife_days_ahead() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 + (MINING_WORTH_HALF_LIFE * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64);
        assert_eq!(worth, MINING_WORTH_MAX);
    }

    #[test]
    fn test_zero_convergence() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 - (1000.0 * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64);
        assert!(worth < 1e-10);
    }

    #[test]
    fn test_two_half_life() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 - (2.0*MINING_WORTH_HALF_LIFE * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64);
        assert_eq!(worth, MINING_WORTH_MAX / 4.0);
    }

}