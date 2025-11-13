
use crate::{accounting::account::TransactionStub, blockchain::{chain::Chain, chain_shard::ChainShard}, nodes::peer::Peer, primitives::{block::{Block, BlockHeader}, transaction::{Transaction, TransactionFilter}}};
use pillar_crypto::{hashing::{HashFunction, Hashable}, proofs::MerkleProof, types::StdByteArray};
use pillar_serialize::PillarSerialize;


/// ==============================================================
/// When adding a new message, update protocol/serialization.rs
/// ==============================================================

#[derive( Debug, Clone)]
pub enum Message {
    // dummy ping
    Ping,
    // discovery
    DiscoveryRequest,
    // discovery response is simply a reponse with own peer
    // used if someone knows ip address but not details like public key
    DiscoveryResponse(Peer),
    // request full chain
    ChainRequest,
    // response with full chain
    ChainResponse(Chain),
    // request for all peers
    PeerRequest,
    // response with all peers
    PeerResponse(Vec<Peer>),
    // inform of who you are
    Declaration(Peer),
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
    ChainSyncRequest(Vec<StdByteArray>),
    // chain syncing response - the blocks that are missing. each chain is the child of leaves that shoudle be kept
    ChainSyncResponse(Vec<Chain>),
    // request for peers filtered between a lower percentile and an upper percentile based on reputation
    PercentileFilteredPeerRequest(f32, f32),
    // response with peers filtered between a lower percentile and an upper percentile based on reputation
    PercentileFilteredPeerResponse(Vec<Peer>),
    // encrypted message
    EncryptedMessage(Vec<u8>),
    // privacy request -- holds public key
    PrivacyRequest(StdByteArray),
    // privacy response, holds public key
    PrivacyResponse(StdByteArray),
    // error message
    Error(String)
}

impl Message{
    pub fn name(&self) -> String {
        match self{
            Message::Ping => "Ping".to_string(),
            Message::DiscoveryRequest => "DiscoveryRequest".to_string(),
            Message::DiscoveryResponse(_) => "DiscoveryResponse".to_string(),
            Message::ChainRequest => "ChainRequest".to_string(),
            Message::ChainResponse(_) => "ChainResponse".to_string(),
            Message::PeerRequest => "PeerRequest".to_string(),
            Message::PeerResponse(_) => "PeerResponse".to_string(),
            Message::Declaration(_) => "Declaration".to_string(),
            Message::TransactionBroadcast(_) => "TransactionBroadcast".to_string(),
            Message::TransactionAck => "TransactionAck".to_string(),
            Message::BlockTransmission(_) => "BlockTransmission".to_string(),
            Message::BlockAck => "BlockAck".to_string(),
            Message::BlockRequest(_) => "BlockRequest".to_string(),
            Message::BlockResponse(_) => "BlockResponse".to_string(),
            Message::ChainShardRequest => "ChainShardRequest".to_string(),
            Message::ChainShardResponse(_) => "ChainShardResponse".to_string(),
            Message::TransactionProofRequest(_) => "TransactionProofRequest".to_string(),
            Message::TransactionProofResponse(_) => "TransactionProofResponse".to_string(),
            Message::TransactionFilterRequest(_, _) => "TransactionFilterRequest".to_string(),
            Message::TransactionFilterAck => "TransactionFilterAck".to_string(),
            Message::TransactionFilterResponse(_, _) => "TransactionFilterResponse".to_string(),
            Message::ChainSyncRequest(_) => "ChainSyncRequest".to_string(),
            Message::ChainSyncResponse(_) => "ChainSyncResponse".to_string(),
            Message::PercentileFilteredPeerRequest(_, _) => "PercentileFilteredPeerRequest".to_string(),
            Message::PercentileFilteredPeerResponse(_) => "PercentileFilteredPeerResponse".to_string(),
            Message::EncryptedMessage(_) => "EncryptedMessage".to_string(),
            Message::PrivacyRequest(_) => "PrivacyRequest".to_string(),
            Message::PrivacyResponse(_) => "PrivacyResponse".to_string(),
            Message::Error(_) => "Error".to_string(),
        }
    }

    pub fn code(&self) -> u8 {
        match self{
            Message::Ping => 0,
            Message::ChainRequest => 1,
            Message::ChainResponse(_) => 2,
            Message::PeerRequest => 3,
            Message::PeerResponse(_) => 4,
            Message::Declaration(_) => 5,
            Message::TransactionBroadcast(_) => 6,
            Message::TransactionAck => 7,
            Message::BlockTransmission(_) => 8,
            Message::BlockAck => 9,
            Message::BlockRequest(_) => 10,
            Message::BlockResponse(_) => 11,
            Message::ChainShardRequest => 12,
            Message::ChainShardResponse(_) => 13,
            Message::TransactionProofRequest(_) => 14,
            Message::TransactionProofResponse(_) => 15,
            Message::TransactionFilterRequest(_, _) => 16,
            Message::TransactionFilterAck => 17,
            Message::TransactionFilterResponse(_, _) => 18,
            Message::ChainSyncRequest(_) => 19,
            Message::ChainSyncResponse(_) => 20,
            Message::PercentileFilteredPeerRequest(_, _) => 21,
            Message::PercentileFilteredPeerResponse(_) => 22,
            Message::Error(_) => 23,
            Message::DiscoveryRequest => 24,
            Message::DiscoveryResponse(_) => 25,
            Message::EncryptedMessage(_) => 26,
            Message::PrivacyRequest(_) => 27,
            Message::PrivacyResponse(_) => 28,
        }
    }

    /// Indicates whether a node should support encryption for this message type
    pub fn encryption_supported(&self) -> bool {
        match self{
            Message::Ping => true,
            _ => false
        }
    }
}

impl Hashable for Message{
    fn hash(&self, hasher: &mut impl HashFunction) -> Result<StdByteArray, std::io::Error> {
        let bin = PillarSerialize::serialize_pillar(self).unwrap();
        hasher.update(bin);
        hasher.digest()
    }
}
