
use core::f64;
use std::collections::{HashMap, HashSet};

use pillar_crypto::types::StdByteArray;

use crate::{accounting::{account::Account, state::StateManager}, blockchain::chain::Chain, nodes::{node::{Broadcaster, Node}, peer::Peer}, primitives::{block::BlockHeader, messages::Message}};

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
/// * `now` - the current time in seconds since epoch
pub fn block_worth_scaling_fn(block_time: u64, current_time: u64) -> f64 {
    // days since epoch
    let current_time = current_time as f64 / 60.0 / 60.0 / 24.0;
    
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
pub fn nth_percentile_peer(lower_n: f32, upper_n: f32, chain: &Chain) -> Vec<StdByteArray>{
    if !(0.0..=1.0).contains(&lower_n)  || !(0.0..=1.0).contains(&upper_n) {
        panic!("n must be between 0 and 1");
    }

    let accounts = chain.state_manager.get_all_accounts(chain.get_state_root().unwrap());
    let mut reputations: Vec<(StdByteArray, f64)> = accounts.iter().map(|a|{
        let rep = &a.history;
        if rep.is_none(){
            return (a.address, 0.0);
        }
        let rep = rep.clone().unwrap();
        (a.address, rep.compute_reputation(chain.get_top_block().unwrap().header.timestamp))
    }).collect::<Vec<_>>();
    reputations.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap()); // this is ordering in ascending order
    let lower_n = (lower_n * reputations.len() as f32).round() as usize;
    let upper_n = (upper_n * reputations.len() as f32).round() as usize;
    let mut peers = Vec::new();
    for i in reputations.iter().skip(lower_n).take(upper_n) {
        peers.push(i.0);
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
                panic!("Expected PercentileFilteredPeerResponse, got {m:?}"); // TODO dont just panic here
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

pub fn get_current_reputations_for_stampers_from_state(
    state_manager: &StateManager,
    previous_header: &BlockHeader,
    header: &BlockHeader,
) -> HashMap<StdByteArray, f64> {
    let stampers = header.tail.get_stampers();
    // get all reputations according to previous block
    stampers.iter().map(|stamper| {
        let history = state_manager.get_account(stamper, previous_header.state_root.unwrap())
            .unwrap_or(Account::new(*stamper, 0)).history;
        if let Some(history) = history {
            // compute based on new block
            (*stamper, history.compute_reputation(header.timestamp))
        } else {
            (*stamper, 0.0) // no history, no reputation
        }
    }).collect()
}

pub fn get_current_reputations_for_stampers(
    chain: &Chain,
    header: &BlockHeader,
) -> HashMap<StdByteArray, f64> {
    let previous_block = chain.get_block(&header.previous_hash).expect("Previous block must exist");
    let state_manager = &chain.state_manager;
    get_current_reputations_for_stampers_from_state(
        state_manager,
        &previous_block.header,
        header,
    )
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
        let worth = block_worth_scaling_fn(
            block_time,
            block_time, 
        );
        assert_eq!(worth, 1.0);
    }

    #[test]
    fn test_block_worth_scaling_fn_halflife_days_ago() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 - (MINING_WORTH_HALF_LIFE * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64, 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
        assert_eq!(worth, MINING_WORTH_MAX / 2.0);
    }

    #[test]
    fn test_block_worth_scaling_fn_halflife_days_ahead() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 + (MINING_WORTH_HALF_LIFE * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64, 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
        assert_eq!(worth, MINING_WORTH_MAX);
    }

    #[test]
    fn test_zero_convergence() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 - (1000.0 * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64, 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
        assert!(worth < 1e-10);
    }

    #[test]
    fn test_two_half_life() {
        let block_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64 - (2.0*MINING_WORTH_HALF_LIFE * 24.0 * 60.0 * 60.0);
        let worth = block_worth_scaling_fn(block_time as u64, 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
        assert_eq!(worth, MINING_WORTH_MAX / 4.0);
    }

}