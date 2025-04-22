use crate::blockchain::chain::Chain;
use serde::{Serialize, Deserialize};
use super::Peer;


#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    ChainRequest,
    ChainResponse(Chain),
    PeerRequest,
    PeerResponse(Vec<Peer>)
}