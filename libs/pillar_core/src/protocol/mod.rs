use crate::protocol::versions::Versions;

pub mod chain;
pub mod peers;
pub mod pow;
pub mod difficulty;
pub mod transactions;
pub mod communication;
pub mod reputation;
pub mod versions;

pub const PROTOCOL_VERSION: Versions = Versions::V1V4;