use crate::{blockchain::chain::Chain, primitives::{block::Block, transaction::Transaction}};
use serde::{Serialize, Deserialize};
use super::peer::Peer;


#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    ChainRequest,
    ChainResponse(Chain),
    PeerRequest,
    PeerResponse(Vec<Peer>),
    Declaration(Peer),
    TransactionRequest(Transaction),
    TransactionAck,
    BlockTransmission(Block),
    BlockAck,
    Error(String)
}