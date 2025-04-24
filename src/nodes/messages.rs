use crate::{blockchain::chain::Chain, primitives::transaction::Transaction};
use serde::{Serialize, Deserialize};
use super::Peer;


#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    ChainRequest,
    ChainResponse(Chain),
    PeerRequest,
    PeerResponse(Vec<Peer>),
    Declaration(Peer),
    TransactionRequest(Transaction),
    TransactionAck,
    Error(String)
}