use std::collections::HashSet;

use crate::{accounting::account::TransactionStub, blockchain::{chain::Chain, chain_shard::ChainShard}, nodes::peer::Peer, primitives::{block::{Block, BlockHeader}, transaction::{Transaction, TransactionFilter}}};
use pillar_crypto::{hashing::{HashFunction, Hashable}, proofs::MerkleProof, serialization::PillarSerialize, types::StdByteArray};
use serde::{Serialize, Deserialize};


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

impl PillarSerialize for Message {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        match self {
            Message::Declaration(_, _) => {
                bincode::serialize(&self).map_err(std::io::Error::other)
            },
            _ => {
                let encoded = bincode::serialize(&self).map_err(std::io::Error::other)?;
                let compressed = lz4_flex::compress_prepend_size(&encoded);
                Ok(compressed)
            }
        }
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        // bincode::deserialize::<Self>(data).map_err(std::io::Error::other)
        let decompressed = lz4_flex::decompress_size_prepended(data).map_err(std::io::Error::other);
        match decompressed {
            Ok(decompressed) => {
                bincode::deserialize::<Self>(&decompressed).map_err(std::io::Error::other)
            },
            Err(_) => {
                // if the decompression fails, try to deserialize without decompression
                bincode::deserialize::<Self>(data).map_err(std::io::Error::other)
            }
        }
    }
}

impl Message{
    pub fn name(&self) -> String {
        match self{
            Message::Ping => "Ping".to_string(),
            Message::ChainRequest => "ChainRequest".to_string(),
            Message::ChainResponse(_) => "ChainResponse".to_string(),
            Message::PeerRequest => "PeerRequest".to_string(),
            Message::PeerResponse(_) => "PeerResponse".to_string(),
            Message::Declaration(_, _) => "Declaration".to_string(),
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
            Message::Error(_) => "Error".to_string(),
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
