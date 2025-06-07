use flume::Receiver;

use crate::{accounting::account::Account, crypto::{hashing::{DefaultHash, HashFunction, Hashable}, signing::{SigFunction, Signable}}, nodes::{messages::Message, node::{Broadcaster, Node, StdByteArray}}, primitives::{block::BlockHeader, transaction::{Transaction}}};

/// Submit a transaction to the network
/// 
/// # Arguments
/// * `node` - The node to submit the transaction to
/// * `sender` - The account sending the transaction
/// * `signer` - The signing key for the sender
/// * `receiver` - The account address for which the transaction is intended
/// * `amount` - The amount to send
/// * `register_completion_callback` - Whether to register a callback to receive a proof of the transaction when it is incorporated into a block
///     
/// # Returns
/// * `Ok(Some(receiver))` - If the transaction was acknowledged and a callback was registered
/// * `Ok(None)` - If the transaction was acknowledged but no callback was registered
/// * `Err(e)` - If the transaction was not acknowledged or an error occurred
pub async fn submit_transaction<const K: usize, const P: usize>(
    node: &mut Node, 
    sender: &mut Account, 
    signer: &mut impl SigFunction<K, P, 64>,
    receiver: StdByteArray,
    amount: u64,
    register_completion_callback: bool,
    timestamp: Option<u64>
) -> Result<Option<Receiver<BlockHeader>>, std::io::Error> {
    let nonce = sender.local_nonce; sender.local_nonce += 1;
    let timestamp = match timestamp{
        Some(t) => t,
        None => {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs()
        }
    };
    let mut transaction = Transaction::new(
        sender.address, 
        receiver, 
        amount, 
        timestamp, 
        nonce, 
        &mut DefaultHash::new()
    );
    // sign with the signer
    transaction.sign(signer);
    // broadcast and wait for peer responses
    let message = Message::TransactionBroadcast(transaction);
    // we do not want this to wait in broadcast queue, so we will lock it out immediately
    node.inner.broadcasted_already.lock().await.insert(message.hash(&mut DefaultHash::new()).unwrap());
    let results = node.broadcast(&message).await?;
    // check if the transaction was acknowledged at least once
    let ok = results.iter().any(|x| {
        matches!(x, Message::TransactionAck)
    });
    match ok{
        true => {
            if register_completion_callback {
                let receiver = node.register_transaction_callback(transaction.into()).await;
                Ok(Some(receiver))
            }else{
                Ok(None)
            }
        },
        false => {
            Err(std::io::Error::other("Transaction not acknowledged"))
        }
    }
}