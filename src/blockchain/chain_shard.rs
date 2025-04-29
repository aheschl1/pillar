use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::primitives::block::BlockHeader;

use super::chain::Chain;

/// chain shard is used to build up a chain given a list of block headers
/// It is responsible for the validation and construction of the chain from a new node.

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChainShard {
    nodes: HashMap<[u8; 32], BlockHeader>,
}
