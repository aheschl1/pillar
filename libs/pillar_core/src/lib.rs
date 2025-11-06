pub mod blockchain;
pub mod nodes;
pub mod primitives;
pub mod protocol;
pub mod accounting;
pub mod reputation;
pub mod persistence;

pub const PROTOCOL_PORT: u16 = 13000;
pub const MAX_BLOCK_DATA_SIZE: usize = 64 * 1024; // 64 KB