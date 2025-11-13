//! Cryptographic primitives used by the Pillar blockchain.
//!
//! This crate provides small, focused building blocks:
//! - Hashing traits and a SHA3-256 default hasher
//! - Merkle tree construction and proofs of inclusion
//! - A compact Merkle trie for state and its proofs
//! - Signing/verification (ed25519) abstractions and defaults
//! - Fixed-size byte array types used across the project
//!
//! None of the public APIs in this crate perform network or filesystem I/O.

/// Reusable hashing traits and a default SHA3-256 hasher.
pub mod hashing;
/// Binary Merkle tree utilities and node types.
pub mod merkle;
/// Signature traits and default ed25519 signer/verifier.
pub mod signing;
/// Compact Merkle trie for key/value state storage.
pub mod merkle_trie;
/// Utilities for generating and verifying Merkle and trie proofs.
pub mod proofs;
/// Common type aliases and constants used by this crate.
pub mod types;

pub mod encryption;