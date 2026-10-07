pub mod blockchain;
pub mod nodes;
pub mod primitives;
pub mod protocol;
pub mod accounting;
pub mod reputation;
pub mod persistence;

pub const PROTOCOL_PORT: u16 = 13000;
/// Account credited at genesis (hex be6570300cada648b4ed31a1d068292c66b51371ad311ca93cb96fffbe199728).
/// Every node rebuilds the genesis state from these, so changing either starts a different chain.
pub const GENESIS_TREASURY_ADDRESS: [u8; 32] = [0xbe, 0x65, 0x70, 0x30, 0x0c, 0xad, 0xa6, 0x48, 0xb4, 0xed, 0x31, 0xa1, 0xd0, 0x68, 0x29, 0x2c, 0x66, 0xb5, 0x13, 0x71, 0xad, 0x31, 0x1c, 0xa9, 0x3c, 0xb9, 0x6f, 0xff, 0xbe, 0x19, 0x97, 0x28];
pub const GENESIS_TREASURY_BALANCE: u64 = 1_000_000_000;
