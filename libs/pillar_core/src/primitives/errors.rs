use std::fmt::Display;

use pillar_crypto::types::StdByteArray;

use crate::primitives::block::BlockHeader;

#[derive(Debug)]
pub enum BlockValidationError {
    /// The block is invalid
    MalformedBlock(String),
    /// malformed shard
    MalformedShard(String),
    /// The block is invalid because it has no miner address
    NoMinerAddress(BlockHeader),
    /// The block is invalid because it has no state root
    NoStateRoot(BlockHeader),
    /// The block is invalid because the hash does not match the header
    HashMismatch(StdByteArray, StdByteArray),
    /// The block is invalid because the difficulty does not match the header
    DifficultyMismatch(u64, BlockHeader),
    /// The block is invalid because the timestamp is in the future
    FutureTimestamp(u64),
    /// The block is invalid because a signature in the tail is invalid
    InvalidStampSignature(StdByteArray),
    /// The block is invalid because it contains an invalid transaction
    InvalidTransaction(String),
    /// The transaction is invalid because it has no sender
    TransactionNoSender,
    /// The transaction is invalid because it has no receiver
    TransactionNoReceiver,
    /// The transaction is invalid because the timestamp is in the future
    TransactionFutureTimestamp,
    /// The transaction is invalid because the nonce is not correct
    TransactionNonceMismatch(u64, u64),
    /// The transaction is invalid because the signature does not match
    TransactionSignatureMismatch,
    /// The transaction is invalid because the sender does not have enough balance
    TransactionInsufficientBalance(u64),
    // invalid transaction signature
    TransactionInvalidSignature,
    // merkle gen
    MerkleGenerationFailed,
    // other
    Other(String),
}

impl Display for BlockValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockValidationError::MalformedBlock(m) => {
                write!(f, "{m}")
            }
            BlockValidationError::NoMinerAddress(header) => {
                write!(f, "Block has no miner address: {header:?}")
            }
            BlockValidationError::NoStateRoot(header) => {
                write!(f, "Block has no state root: {header:?}")
            }
            BlockValidationError::HashMismatch(expected, actual) => {
                write!(f, "Block hash mismatch: expected {expected:?}, got {actual:?}")
            }
            BlockValidationError::DifficultyMismatch(expected, header) => {
                write!(f, "Block difficulty mismatch: expected {expected}, got {header:?}")
            }
            BlockValidationError::FutureTimestamp(timestamp) => {
                write!(f, "Block timestamp is in the future: {timestamp}")
            }
            BlockValidationError::InvalidStampSignature(address) => {
                write!(f, "Invalid tail signature from address: {address:?}")
            }
            BlockValidationError::InvalidTransaction(reason) => {
                write!(f, "Block contains an invalid transaction: {reason}")
            }
            BlockValidationError::TransactionNoSender => {
                write!(f, "Transaction has no sender")
            }
            BlockValidationError::TransactionNoReceiver => {
                write!(f, "Transaction has no receiver")
            }
            BlockValidationError::TransactionFutureTimestamp => {
                write!(f, "Transaction timestamp is in the future")
            }
            BlockValidationError::TransactionNonceMismatch(expected, actual) => {
                write!(f, "Transaction nonce mismatch: expected {expected}, got {actual}")
            }
            BlockValidationError::TransactionSignatureMismatch => {
                write!(f, "Transaction signature does not match")
            }
            BlockValidationError::TransactionInsufficientBalance(balance) => {
                write!(f, "Transaction has insufficient balance: {balance}")
            }
            BlockValidationError::TransactionInvalidSignature => {
                write!(f, "Transaction has an invalid signature")
            },
            BlockValidationError::MalformedShard(reason) => {
                write!(f, "Malformed shard: {reason}")
            }
            BlockValidationError::Other(reason) => {
                write!(f, "Block validation error: {reason}")
            },
            BlockValidationError::MerkleGenerationFailed => {
                write!(f, "Failed to generate Merkle tree")
            }
        }
    }
}

#[derive(Debug)]
pub enum QueryError{
    /// The query is invalid
    InvalidResponse,
    /// bad response
    BadBlock(BlockValidationError),
    /// no reply
    NoReply,
    /// std err
    IOError(std::io::Error),
    /// insufficient info
    InsufficientInfo(String),
}

impl Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryError::InvalidResponse => write!(f, "Invalid response"),
            QueryError::BadBlock(err) => write!(f, "Bad block: {err}"),
            QueryError::NoReply => write!(f, "No reply received"),
            QueryError::IOError(err) => write!(f, "IO error: {err}"),
            QueryError::InsufficientInfo(info) => write!(f, "Insufficient info: {info}"),
        }
    }
}