use serde::{Deserialize, Serialize};
use crate::crypto::{hashing::{HashFunction, Hashable}, signing::Signable};
use serde_with::{serde_as, Bytes};

use super::block::Block;


#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Transaction{
    // header is the header of the transaction
    pub header: TransactionHeader,
    // hash is the sha3_256 hash of the transaction header
    pub hash: [u8; 32],
    // signature is the signature over the transaction header
    #[serde_as(as = "Option<Bytes>")]
    pub signature: Option<[u8; 64]>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TransactionHeader{
    // sender is the ed25519 public key of the sender
    pub sender: [u8; 32],
    // receiver is the ed25519 public key of the receiver
    pub receiver: [u8; 32],
    // amount is the amount of tokens being transferred
    pub amount: u64,
    // timestamp is the time the transaction was created
    pub timestamp: u64,
    // the nonce is a random number used to prevent replay attacks
    pub nonce: u64
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
/// TransactionFilter is sent from lightweight nodes to full nodes in order to register a callback to receive 
/// a proof of a transaction when it is incorporated into a block.
pub struct TransactionFilter {
    // sender is the ed25519 public key of the sender
    pub sender: Option<[u8; 32]>,
    // receiver is the ed25519 public key of the receiver
    pub receiver: Option<[u8; 32]>,
    // amount is the amount of tokens being transferred
    pub amount: Option<u64>,
}

impl TransactionFilter{
    /// Create a new transaction filter
    /// 
    /// # Arguments
    /// 
    /// * `sender` - The sender's public key
    /// * `receiver` - The receiver's public key
    /// * `amount` - The amount of tokens being transferred
    pub fn new(sender: Option<[u8; 32]>, receiver: Option<[u8; 32]>, amount: Option<u64>) -> Self {
        TransactionFilter {
            sender,
            receiver,
            amount,
        }
    }
}

pub trait FilterMatch<T>{
    fn matches(&self, other: &T) -> bool;
}


/// match a transaction to a transaction filter
impl FilterMatch<Transaction> for TransactionFilter {
    fn matches(&self, other: &Transaction) -> bool {
        if let Some(sender) = self.sender {
            if sender != other.header.sender {
                return false;
            }
        }
        if let Some(receiver) = self.receiver {
            if receiver != other.header.receiver {
                return false;
            }
        }
        if let Some(amount) = self.amount {
            if amount != other.header.amount {
                return false;
            }
        }
        true
    }
}

/// match a block to a transaction filter
impl FilterMatch<Block> for TransactionFilter {
    fn matches(&self, other: &Block) -> bool {
        for transaction in &other.transactions {
            if self.matches(transaction) {
                return true;
            }
        }
        false
    }
}

impl From<Transaction> for TransactionFilter{
    fn from(transaction: Transaction) -> Self {
        TransactionFilter {
            sender: Some(transaction.header.sender),
            receiver: Some(transaction.header.receiver),
            amount: Some(transaction.header.amount),
        }
    }
}

impl TransactionHeader {
    pub fn new(
        sender: [u8; 32], 
        receiver: [u8; 32], 
        amount: u64, 
        timestamp: u64, 
        nonce: u64
    ) -> Self {
        TransactionHeader {
            sender,
            receiver,
            amount,
            timestamp,
            nonce
        }
    }

    /// Hash the transaction header using the provided HashFunction
    ///
    /// # Arguments
    ///
    /// * `hasher` - A mutable instance of a type implementing the HashFunction trait
    ///
    /// # Returns
    ///
    /// * The hash of the transaction header as a [u8; 32] array
    pub fn hash(&self, hasher: &mut impl HashFunction) -> [u8; 32] {
        hasher.update(self.sender);
        hasher.update(self.receiver);
        hasher.update(self.amount.to_le_bytes());
        hasher.update(self.timestamp.to_le_bytes());
        hasher.update(self.nonce.to_le_bytes());
        hasher.digest().expect("Hashing failed")
    }
}

impl Transaction {
    /// Create a new transaction
    /// 
    /// # Arguments
    /// 
    /// * `sender` - The sender's public key
    /// * `receiver` - The receiver's public key
    /// * `amount` - The amount of tokens being transferred
    /// * `timestamp` - The time the transaction was created
    /// * `nonce` - A random number used to prevent replay attacks
    pub fn new(
        sender: [u8; 32],
        receiver: [u8; 32],
        amount: u64,
        timestamp: u64,
        nonce: u64,
        hash_function: &mut impl HashFunction
    ) -> Self {
        let header = TransactionHeader::new(sender, receiver, amount, timestamp, nonce);
        let hash = header.hash(hash_function);
        Transaction {
            header,
            hash,
            signature: None,
        }
    }
}

impl Hashable for Transaction {
    fn hash(&self, hasher: &mut impl HashFunction) -> Result<[u8; 32], std::io::Error> {
        Ok(self.header.hash(hasher))
    }
}

impl Into<[u8; 32]> for Transaction {
    fn into(self) -> [u8; 32] {
        self.hash
    }
}


/**
 * if let Some(_) = self.signature {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Transaction already signed",
            ));
        }
        let signature = signer.sign(&self.hash);
        self.signature = Some(signature.to_bytes());
        Ok(())
 */


impl Signable<64> for Transaction {
    
    fn get_signing_bytes(&self) -> impl AsRef<[u8]> {
        &self.hash
    }
    
    fn sign<const K: usize, const P: usize>(&mut self, signing_function: &mut impl crate::crypto::signing::SigFunction<K, P, 64>) -> [u8; 64]{
        if let Some(_) = self.signature {
            panic!("Transaction already signed");
        }
        let signature = signing_function.sign(self);
        self.signature = Some(signature);
        self.signature.unwrap()
    }
}