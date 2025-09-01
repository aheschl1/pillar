//! Common type aliases used across cryptographic components.
/// Standard byte array length used for hashes and keys (32 bytes).
pub const STANDARD_ARRAY_LENGTH: usize = 32;
/// Fixed-size 32-byte array (commonly used for hashes and public keys).
pub type StdByteArray = [u8; STANDARD_ARRAY_LENGTH];
