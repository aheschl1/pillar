use ed25519_dalek::SigningKey;

use crate::{accounting::account::Account, crypto::hashing::{DefaultHash, HashFunction}, nodes::{messages::Message, node::{Broadcaster, Node}}, primitives::transaction::{self, Transaction}};

/// Submit a transaction to the network
/// 
/// # Arguments
/// * `node` - The node to submit the transaction to
/// * `sender` - The account sending the transaction
/// * `signer` - The signing key for the sender
/// * `receiver` - The account address for which the transaction is intended
/// * `amount` - The amount to send
pub async fn submit_transaction(
    node: &Node, 
    sender: &mut Account, 
    signer: SigningKey,
    receiver: [u8; 32], 
    amount: u64
) -> Result<(), std::io::Error> {
    let nonce = sender.nonce; sender.nonce += 1;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();
    let mut transaction = Transaction::new(
        sender.address, 
        receiver, 
        amount, 
        timestamp, 
        nonce, 
        &mut DefaultHash::new()
    );
    // sign with the signer
    transaction.sign(&signer)?;
    // broadcast and wait for peer responses
    let results = node.broadcast(&Message::TransactionRequest(transaction.clone())).await?;
    // check if the transaction was acknowledged at least once
    let ok = results.iter().any(|x| {
        match x {
            Message::TransactionAck => true,
            _ => false
        }
    });
    match ok{
        true => {
            Ok(())
        },
        false => {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Transaction not acknowledged",
            ))
        }
    }
}