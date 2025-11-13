//! Common type aliases used across cryptographic components.
use pillar_serialize::StdByteArray as StdByteArraySerialize;
use pillar_serialize::STANDARD_ARRAY_LENGTH as SERIALIZE_STANDARD_ARRAY_LENGTH;

// Re-exporting types and constants for consistency across the codebase.
/// Standard byte array length used for hashes and keys (32 bytes).
pub const STANDARD_ARRAY_LENGTH: usize = SERIALIZE_STANDARD_ARRAY_LENGTH;
/// Fixed-size 32-byte array (commonly used for hashes and public keys).
pub type StdByteArray = StdByteArraySerialize;
