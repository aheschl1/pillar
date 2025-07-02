use std::collections::HashSet;

use crate::{accounting::account::TransactionStub, blockchain::{chain::Chain, chain_shard::ChainShard}, nodes::peer::Peer, primitives::{block::{Block, BlockHeader}, transaction::{Transaction, TransactionFilter}}};
use pillar_crypto::{hashing::{HashFunction, Hashable}, proofs::MerkleProof, types::StdByteArray};
use serde::{Serialize, Deserialize};
use pillar_crypto::serialization::serialize;


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
    BlockRequest(StdByteArray),
    // response with a specific block
    BlockResponse(Option<Block>),
    // request for the block headers
    ChainShardRequest,
    // response with the block headers
    ChainShardResponse(ChainShard),
    // a request for proof over a transaction
    TransactionProofRequest(TransactionStub),
    // a reponse for the proof. includes block header for hash verification, and merkle proof
    TransactionProofResponse(MerkleProof),
    /// register a transaction filter
    TransactionFilterRequest(TransactionFilter, Peer),
    /// An acknowledgement of a transaction filter
    TransactionFilterAck,
    /// A response to a hit on the transaction filter - with the block header that contains the transaction
    TransactionFilterResponse(TransactionFilter, BlockHeader),
    // chain syncing request - the current leaf hashes of the chain
    ChainSyncRequest(HashSet<StdByteArray>),
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
    fn hash(&self, hasher: &mut impl HashFunction) -> Result<StdByteArray, std::io::Error> {
        let bin = serialize(self).unwrap();
        hasher.update(bin);
        hasher.digest()
    }
}

pub enum Versions{
    V1V4 = 1,
    #[allow(dead_code)]
    V1V6 = 2,
}
/// Returns the expected bincode-encoded size in bytes for a `Message::Declaration(Peer, u64)`
/// under version-specific assumptions (IPv4 or IPv6). Assumes default bincode config.
pub const fn get_declaration_length(version: Versions) -> u64 {
    match version {
        Versions::V1V4 => 50, // enum tag + public key + IP tag + IPv4 + port + u32
        Versions::V1V6 => 62, // enum tag + public key + IP tag + IPv6 + port + u32
    }
}

mod tests{
    use pillar_crypto::serialization::serialize_no_compress;

    use crate::{nodes::peer::Peer, primitives::messages::{get_declaration_length, Message, Versions}};


    #[test]
    fn test_declaration_length() {
        let declaration = Message::Declaration(Peer::new(
            [0; 32], 
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)), 
            8000), 
            1
        );

        let declarationv1v6 = Message::Declaration(Peer::new(
            [0; 32], 
            std::net::IpAddr::V6(std::net::Ipv6Addr::new(0, 2, 0, 0, 0, 0, 0, 1)), 
            8000), 
            1
        );
        assert_eq!(get_declaration_length(Versions::V1V4), serialize_no_compress(declaration).unwrap().len() as u64);
        assert_eq!(get_declaration_length(Versions::V1V6), serialize_no_compress(declarationv1v6).unwrap().len() as u64);
    }
}