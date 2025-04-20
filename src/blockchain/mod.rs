pub mod transaction;
pub mod block;


#[cfg(test)]
mod tests{
    use crate::blockchain::{block::BlockHeader, transaction::{Transaction, TransactionHeader}};
    use ed25519_dalek::{Verifier, Signature, SigningKey};
    use rand_core::OsRng;
    // use rand::rngs::OsRng;

    #[test]
    fn test_block_header_hash() {
        let previous_hash = [0u8; 32];
        let merkle_root = [1u8; 32];
        let nonce = 12345;
        let timestamp = 1622547800;

        let block_header = BlockHeader::new(previous_hash, merkle_root, nonce, timestamp);
        let hash = block_header.hash();

        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_transaction_header_hash() {
        let sender = [0u8; 32];
        let receiver = [1u8; 32];
        let amount = 100;
        let timestamp = 1622547800;
        let nonce = 12345;

        let transaction_header = TransactionHeader::new(sender, receiver, amount, timestamp, nonce);
        let hash = transaction_header.hash();

        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_transaction_sign() {
        let sender = [0u8; 32];
        let receiver = [1u8; 32];
        let amount = 100;
        let timestamp = 1622547800;
        let nonce = 12345;

        let mut transaction = Transaction::new(sender, receiver, amount, timestamp, nonce);
        assert!(transaction.signature.is_none());
        
        let signing_key = SigningKey::generate(&mut OsRng);

        transaction.sign(&signing_key).unwrap();
        assert!(transaction.signature.is_some());
        // Verify the signature
        signing_key.verifying_key()
            .verify(&transaction.hash, &Signature::from_bytes(&transaction.signature.unwrap()))
            .expect("Signature verification failed");
    }
}