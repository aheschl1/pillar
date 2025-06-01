use core::hash;
use std::{collections::HashSet, hash::Hash, net::{IpAddr, Ipv4Addr, Ipv6Addr}};

use crate::{accounting::account::TransactionStub, blockchain::{chain::Chain, chain_shard::ChainShard}, crypto::{hashing::{DefaultHash, HashFunction, Hashable}, merkle::MerkleProof}, primitives::{block::{Block, BlockHeader}, transaction::{Transaction, TransactionFilter}}};
use serde::{Serialize, Deserialize};
use super::peer::Peer;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    // dummy ping
    Ping,
    // request full chain
    ChainRequest,
    // response with full chain
    ChainResponse(Chain),
    // request for all peers
    PeerRequest,
    // response with all peers
    PeerResponse(Vec<Peer>),
    // inform of who you are and message length following - always the first message
    Declaration(Peer, u32),
    // request for a transaction
    TransactionBroadcast(Transaction),
    // acknowledge a transaction has been received
    TransactionAck,
    // transmit a newly mined block
    BlockTransmission(Block),
    // acknowledge a block has been received
    BlockAck,
    // request for a specific block
    BlockRequest([u8; 32]),
    // response with a specific block
    BlockResponse(Option<Block>),
    // request for the block headers
    ChainShardRequest,
    // response with the block headers
    ChainShardResponse(ChainShard),
    // a request for proof over a transaction
    TransactionProofRequest(TransactionStub),
    // a reponse for the proof. includes block header for hash verification, and merkle proof
    TransactionProofResponse(MerkleProof, BlockHeader),
    /// register a transaction filter
    TransactionFilterRequest(TransactionFilter, Peer),
    /// An acknowledgement of a transaction filter
    TransactionFilterAck,
    /// A response to a hit on the transaction filter - with the block header that contains the transaction
    TransactionFilterResponse(TransactionFilter, BlockHeader),
    // chain syncing request - the current leaf hashes of the chain
    ChainSyncRequest(HashSet<[u8; 32]>),
    // chain syncing response - the blocks that are missing. each chain is the child of leaves that shoudle be kept
    ChainSyncResponse(Vec<Chain>),
    // request for peers filtered between a lower percentile and an upper percentile based on reputation
    PercentileFilteredPeerRequest(f32, f32),
    // response with peers filtered between a lower percentile and an upper percentile based on reputation
    PercentileFilteredPeerResponse(Vec<Peer>),
    // error message
    Error(String)
}

impl Hashable for Message{
    fn hash(&self, hasher: &mut impl HashFunction) -> Result<[u8; 32], std::io::Error> {
        let bin = bincode::serialize(self).unwrap();
        hasher.update(bin);
        hasher.digest()
    }
}

pub enum Versions{
    V1V4 = 1,
    V1V6 = 2,
}
/// Returns the expected bincode-encoded size in bytes for a `Message::Declaration(Peer, u64)`
/// under version-specific assumptions (IPv4 or IPv6). Assumes default bincode config.
pub const fn get_declaration_length(version: Versions) -> u64 {
    match version {
        Versions::V1V4 => 50, // enum tag + public key + IP tag + IPv4 + port + u32
        Versions::V1V6 => 1 + 32 + 1 + 16 + 2 + 4, // enum tag + public key + IP tag + IPv6 + port + u32
    }
}