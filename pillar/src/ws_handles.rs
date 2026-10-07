use axum::extract::ws::{Message, WebSocket};
use pillar_core::{accounting::wallet::Wallet, nodes::node::Node, protocol::transactions::submit_transaction};
use crate::run::{remember_sent, SentTransactions};
use pillar_crypto::types::StdByteArray;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ============================= Transaction Handling =============================

#[derive(Serialize, Deserialize)]
pub(crate) struct TransactionPost {
    receiver: StdByteArray,
    amount: u64,
    register_completion_callback: bool
}

#[derive(Serialize, Deserialize)]
struct TransactionResponse {
    success: bool,
    message: String,
    transaction_hash: Option<StdByteArray>,
    keep_alive: bool
}

/// Handle a transaction post request from the client
/// - Validates the request
/// - Submits the transaction to the node
/// - Sends a response back to the client
/// - If `register_completion_callback` is true, waits for the transaction to be included in
/// a block and sends a completion message back to the client
pub(crate) async fn handle_transaction_post(
    websocket: &mut WebSocket,
    request: TransactionPost,
    node: &mut Node,
    wallet: &RwLock<Wallet>,
    sent: &SentTransactions
){
    tracing::info!("Handling transaction post");

    // the wallet is held only while sending (so payments get consecutive nonces), not while
    // waiting for the block, which can take as long as it takes; /wallet, /node, the faucet
    // and the periodic save all need it
    let result = submit_transaction(
        node,
        &mut *wallet.write().await,
        request.receiver,
        request.amount,
        request.register_completion_callback,
        None
    ).await;

    match result {
        Ok(tx) => {
            remember_sent(sent, tx.1);
            let message = TransactionResponse {
                success: true,
                message: "Transaction submitted successfully".to_string(),
                transaction_hash: Some(tx.1.hash),
                keep_alive: request.register_completion_callback
            };
            // sends fail only when the client has gone, which ends this handler anyway
            let _ = websocket.send(Message::Text(serde_json::to_string(&message).unwrap().into())).await;
            if let Some(callback) = tx.0 {
                let Ok(cb) = callback.recv_async().await else {
                    // the callback was replaced by another registration for the same filter
                    tracing::warn!("Lost the completion callback for {:?}", tx.1.hash);
                    return;
                };
                let response = TransactionResponse {
                    success: true,
                    message: format!("Transaction {:?} completed in block: {:?}", tx.1.hash, cb.completion.as_ref().unwrap().hash),
                    transaction_hash: Some(tx.1.hash),
                    keep_alive: false
                };
                let _ = websocket.send(Message::Text(serde_json::to_string(&response).unwrap().into())).await;
                tracing::info!("Transaction {:?} completed in block: {:?}", tx.1.hash, cb.completion.as_ref().unwrap().hash);
            }
        },
        Err(e) => {
            let response = TransactionResponse {
                success: false,
                message: format!("Failed to submit transaction: {e:?}"),
                transaction_hash: None,
                keep_alive: false
            };
            let _ = websocket.send(Message::Text(serde_json::to_string(&response).unwrap().into())).await;
        }
    };
}
