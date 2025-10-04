use axum::extract::ws::{Message, WebSocket};
use pillar_core::{accounting::wallet::Wallet, nodes::node::Node, protocol::transactions::submit_transaction};
use pillar_crypto::types::StdByteArray;
use serde::{Deserialize, Serialize};

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
    wallet: &mut Wallet
){
    tracing::info!("Handling transaction post");

    let result = submit_transaction(
        node,
        wallet,
        request.receiver,
        request.amount,
        request.register_completion_callback,
        None
    ).await;

    match result {
        Ok(tx) => {
            let message = TransactionResponse {
                success: true,
                message: "Transaction submitted successfully".to_string(),
                transaction_hash: Some(tx.1.hash),
                keep_alive: request.register_completion_callback
            };
            websocket.send(Message::Text(serde_json::to_string(&message).unwrap().into())).await.unwrap();
            if let Some(callback) = tx.0 {
                let cb = callback.recv_async().await.unwrap();
                let response = TransactionResponse {
                    success: true,
                    message: format!("Transaction {:?} completed in block: {:?}", tx.1.hash, cb.completion.as_ref().unwrap().hash),
                    transaction_hash: Some(tx.1.hash),
                    keep_alive: false
                };
                websocket.send(Message::Text(serde_json::to_string(&response).unwrap().into())).await.unwrap();
                tracing::info!("Transaction {:?} completed in block: {:?}", tx.1.hash, cb.completion.as_ref().unwrap().hash);
            }
        },
        Err(e) => {
            let response = TransactionResponse {
                success: false,
                message: format!("Failed to submit transaction: {:?}", e),
                transaction_hash: None,
                keep_alive: false
            };
            websocket.send(Message::Text(serde_json::to_string(&response).unwrap().into())).await.unwrap();
        }
    };
}
