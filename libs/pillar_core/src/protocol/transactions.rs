use flume::Receiver;
use pillar_crypto::{hashing::{DefaultHash, Hashable}, proofs::verify_proof_of_inclusion, signing::{SigFunction, Signable}, types::StdByteArray};
use tracing::instrument;

use crate::{accounting::{account::TransactionStub, wallet::Wallet}, nodes::node::{Broadcaster, Node}, primitives::{block::BlockHeader, errors::QueryError, messages::Message, transaction::Transaction}};

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
pub async fn submit_transaction(
    node: &mut Node, 
    wallet: &mut Wallet,
    receiver: StdByteArray,
    amount: u64,
    register_completion_callback: bool,
    timestamp: Option<u64>
) -> Result<(Option<Receiver<BlockHeader>>, Transaction), QueryError> {
    let wallet_nonce = wallet.nonce_mut();
    let nonce = *wallet_nonce; *wallet_nonce += 1;

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
        wallet.address, 
        receiver, 
        amount, 
        timestamp, 
        nonce, 
        &mut DefaultHash::new()
    );
    // sign with the signer

    transaction.sign(wallet);

    // broadcast and wait for peer responses
    let message = Message::TransactionBroadcast(transaction);
    // we do not want this to wait in broadcast queue, so we will lock it out immediately
    node.inner.broadcasted_already.write().await.insert(message.hash(&mut DefaultHash::new()).unwrap());
    let results = node.broadcast(&message).await.map_err(|e| {
        tracing::error!("Failed to broadcast transaction: {}", e);
        QueryError::NoReply
    })?;
    // check if the transaction was acknowledged at least once
    let ok = results.iter().any(|x| {
        matches!(x, Message::TransactionAck)
    });
    match ok{
        true => {
            if register_completion_callback {
                let receiver = node.register_transaction_callback(transaction.into()).await;
                Ok((Some(receiver), transaction))
            }else{
                Ok((None, transaction))
            }
        },
        false => {
            Err(QueryError::NoReply)
        }
    }
}

#[instrument(skip(node, header))]
pub async fn get_transaction_proof(node: &mut Node, transaction: &Transaction, header: &BlockHeader) -> bool{
    let message = Message::TransactionProofRequest(TransactionStub { 
        block_hash: header.hash(&mut DefaultHash::new()).unwrap(), 
        transaction_hash: transaction.hash });

    let results = node.broadcast(&message).await.unwrap();
    for (i, result) in results.iter().enumerate() {
        if let Message::TransactionProofResponse(proof) = result
            && verify_proof_of_inclusion(
                *transaction,
                proof,
                header.merkle_root,
                &mut DefaultHash::new()
            ){
                tracing::info!("Transaction proof verified by peer {}", i);
                return true;
            }
    }
    tracing::info!("No proof available, with {} responses.", results.len());
    return false;
}