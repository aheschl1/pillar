use ed25519::signature::Signer;
use crate::crypto::hashing::{HashFunction, Hashable};

pub struct Transaction{
    // header is the header of the transaction
    pub header: TransactionHeader,
    // hash is the sha3_256 hash of the transaction header
    pub hash: [u8; 32],
    // signature is the signature over the transaction header
    pub signature: Option<[u8; 64]>,
}

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
    /// Sign the transaction with the given signer
    /// 
    /// # Arguments
    /// 
    /// * `signer` - The signer to use to sign the transaction
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if the transaction was signed successfully
    /// * `Err(std::io::Error)` if the transaction was already signed
    pub fn sign(&mut self, signer: & impl Signer<ed25519::Signature>) -> Result<(), std::io::Error> {
        if let Some(_) = self.signature {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Transaction already signed",
            ));
        }
        let signature = signer.sign(&self.hash);
        self.signature = Some(signature.to_bytes());
        Ok(())
    }
}

impl Hashable for Transaction {
    fn hash(&self, hasher: &mut impl HashFunction) -> [u8; 32] {
        self.header.hash(hasher)
    }
}