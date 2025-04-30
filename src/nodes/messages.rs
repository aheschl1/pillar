use crate::{blockchain::{chain::Chain, chain_shard::ChainShard}, primitives::{block::{Block, BlockHeader}, transaction::Transaction}};
use serde::{Serialize, Deserialize};
use super::peer::Peer;


#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    // request full chain
    ChainRequest,
    // response with full chain
    ChainResponse(Chain),
    // request for all peers
    PeerRequest,
    // response with all peers
    PeerResponse(Vec<Peer>),
    // inform of who you are - always the first message
    Declaration(Peer),
    // request for a transaction
    TransactionRequest(Transaction),
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
    // error message
    Error(String)
}