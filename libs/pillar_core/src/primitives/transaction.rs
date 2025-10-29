use bytemuck::{Pod, Zeroable};
use pillar_crypto::{hashing::{HashFunction, Hashable}, signing::{SigFunction, Signable}, types::StdByteArray};

use super::block::Block;


#[derive(Debug,  Clone, Hash, PartialEq, Eq)]
pub struct Transaction{
    // header is the header of the transaction
    pub header: TransactionHeader,
    // hash is the sha3_256 hash of the transaction header
    pub hash: StdByteArray,
    // signature is the signature over the transaction header
    pub signature: [u8; 64],
}

#[derive(Pod, Zeroable, Debug,  Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C, align(8))]
pub(crate) struct TransactionMeta{
    // sender is the ed25519 public key of the sender
    pub sender: StdByteArray,
    // receiver is the ed25519 public key of the receiver
    pub receiver: StdByteArray,
    // amount is the amount of tokens being transferred
    pub amount: u64,
    // timestamp is the time the transaction was created
    pub timestamp: u64,
    // the nonce is a random number used to prevent replay attacks
    pub nonce: u64,
}

#[derive(Debug,  Clone, Hash, PartialEq, Eq)]
pub struct TransactionHeader{
    pub(crate) meta: TransactionMeta,
    // data field
    pub data: Vec<u8>,
}

#[derive(Debug,  Clone, PartialEq, Eq, Hash)]
/// TransactionFilter is sent from lightweight nodes to full nodes in order to register a callback to receive 
/// a proof of a transaction when it is incorporated into a block.
pub struct TransactionFilter {
    // sender is the ed25519 public key of the sender
    pub sender: Option<StdByteArray>,
    // receiver is the ed25519 public key of the receiver
    pub receiver: Option<StdByteArray>,
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
    pub fn new(sender: Option<StdByteArray>, receiver: Option<StdByteArray>, amount: Option<u64>) -> Self {
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
        if let Some(sender) = self.sender
            && sender != other.header.meta.sender {
                return false;
            }
        if let Some(receiver) = self.receiver
            && receiver != other.header.meta.receiver {
                return false;
            }
        if let Some(amount) = self.amount
            && amount != other.header.meta.amount {
                return false;
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
            sender: Some(transaction.header.meta.sender),
            receiver: Some(transaction.header.meta.receiver),
            amount: Some(transaction.header.meta.amount),
        }
    }
}

impl TransactionHeader {
    pub fn new(
        sender: StdByteArray, 
        receiver: StdByteArray, 
        amount: u64, 
        timestamp: u64, 
        nonce: u64
    ) -> Self {
        TransactionHeader {
            meta: TransactionMeta {
                sender,
                receiver,
                amount,
                timestamp,
                nonce
            },
            data: Vec::with_capacity(0)
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
    /// * The hash of the transaction header as a StdByteArray array
    pub fn hash(&self, hasher: &mut impl HashFunction) -> StdByteArray {
        hasher.update(self.meta.sender);
        hasher.update(self.meta.receiver);
        hasher.update(self.meta.amount.to_le_bytes());
        hasher.update(self.meta.timestamp.to_le_bytes());
        hasher.update(self.meta.nonce.to_le_bytes());
        hasher.update(&self.data);
        hasher.digest().expect("Hashing failed")
    }


    pub fn ammount(&self) -> u64 {
        self.meta.amount
    }

    pub fn sender(&self) -> StdByteArray {
        self.meta.sender
    }

    pub fn receiver(&self) -> StdByteArray {
        self.meta.receiver
    }

    pub fn timestamp(&self) -> u64 {
        self.meta.timestamp
    }

    pub fn nonce(&self) -> u64 {
        self.meta.nonce
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
        sender: StdByteArray,
        receiver: StdByteArray,
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
            signature: [0; 64],
        }
    }
}

impl Hashable for Transaction {
    fn hash(&self, hasher: &mut impl HashFunction) -> Result<StdByteArray, std::io::Error> {
        Ok(self.header.hash(hasher))
    }
}

impl From<Transaction> for StdByteArray {
    fn from(transaction: Transaction) -> Self {
        transaction.hash
    }
}

impl Signable<64> for Transaction {
    
    fn get_signing_bytes(&self) -> impl AsRef<[u8]> {
        &self.hash
    }
    
    fn sign<const K: usize, const P: usize>(&mut self, signing_function: &mut impl SigFunction<K, P, 64>) -> [u8; 64]{
        let signature = signing_function.sign(self);
        self.signature = signature;
        self.signature
    }
}