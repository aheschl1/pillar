use crate::{blockchain::chain_shard::ChainShard, primitives::block::BlockHeader};

/// This is a TODO module

/// The reputation structure holds all the information needed to compute the reputation of a node
/// This information should be stored by each node, and each node can add it to a side chain
pub struct NodeHistory{
    /// The public key of the node
    pub public_key: [u8; 32],
    /// The blocks that have been mined by the node - could be empty if the node does not mine
    pub blocks_mined: Vec<BlockHeader>,
    /// The timestamp of the last block mined
    pub last_update: Option<u64>,
    /// peer distributions - timestamps
    pub peer_distribution: Vec<u64>,
    /// timestamps of block distributions to new nodes - without errors
    pub block_distributions: Vec<u64>,
    /// timestamps of block distributions to new nodes - with errors
    pub incorrect_block_distributions: Vec<u64>
}

impl NodeHistory{
    pub fn new(
        public_key: [u8; 32],
        blocks_mined: Vec<BlockHeader>,
        last_update: Option<u64>,
        peer_distribution: Vec<u64>,
        block_distributions: Vec<u64>,
        incorrect_block_distributions: Vec<u64>
    ) -> Self {
        NodeHistory {
            public_key,
            blocks_mined,
            last_update,
            peer_distribution,
            block_distributions,
            incorrect_block_distributions
        }
    }
    /// Returns the reputation of the node
    pub fn reputation(shard: &ChainShard){
    }
}