pub mod transaction;
pub mod block;
pub mod pool;
pub mod messages;
pub mod errors;
pub mod implementations;

#[cfg(test)]
mod tests{
    use std::num::NonZeroU64;

    use pillar_crypto::{hashing::{DefaultHash, Hashable}, signing::{DefaultSigner, SigFunction, SigVerFunction, Signable}};
    

    use crate::{primitives::{block::{BlockHeader, BlockTail}, transaction::{Transaction, TransactionHeader}}};
    

    #[test]
    fn test_block_header_hash() {
        let previous_hash = [0u8; 32];
        let merkle_root = [1u8; 32];
        let miner_address = [2u8; 32];
        let nonce = 12345;
        let timestamp = 1622547800;
        let state_root = [3u8; 32];

        let block_header = BlockHeader::new(
            None,
            previous_hash, 
            merkle_root, 
            Some(state_root), 
            nonce, timestamp, 
            Some(miner_address), 
            BlockTail::default(), 
            0,
            Some(NonZeroU64::new(1).unwrap()),
        );
        let hash = block_header.hash(&mut DefaultHash::new());

        assert_eq!(hash.unwrap().len(), 32);
    }

    #[test]
    fn test_transaction_header_hash() {
        let sender = [0u8; 32];
        let receiver = [1u8; 32];
        let amount = 100;
        let timestamp = 1622547800;
        let nonce = 12345;

        let transaction_header = TransactionHeader::new(sender, receiver, amount, timestamp, nonce);
        let hash = transaction_header.hash(&mut DefaultHash::new());

        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_transaction_sign() {
        let sender = [0u8; 32];
        let receiver = [1u8; 32];
        let amount = 100;
        let timestamp = 1622547800;
        let nonce = 12345;

        let mut hash_function = DefaultHash::new();

        let mut transaction = Transaction::new(sender, receiver, amount, timestamp, nonce, &mut hash_function);
        assert!(transaction.signature.is_none());
        
        let mut signing_key = DefaultSigner::generate_random();

        transaction.sign(&mut signing_key);
        assert!(transaction.signature.is_some());
        // Verify the signature
        signing_key.get_verifying_function().verify(&transaction.signature.unwrap(), &transaction);

        let transaction2 = Transaction::new(sender, receiver, amount, timestamp, nonce+1, &mut hash_function);
        assert!(!signing_key.get_verifying_function().verify(&transaction.signature.unwrap(), &transaction2));
    }
}