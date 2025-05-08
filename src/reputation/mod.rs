use crate::primitives::block::BlockHeader;



pub struct Reputation{
    /// The public key of the node
    pub public_key: [u8; 32],
    /// The number of blocks mined
    pub blocks_mined: Vec<BlockHeader>,
    /// The timestamp of the last block mined
    pub last_update: u64,
}

