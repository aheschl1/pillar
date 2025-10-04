use axum::{extract::State, http::HeaderMap, response::IntoResponse, routing::get, Router};
use pillar_core::{nodes::node::Node, protocol::transactions::submit_transaction};
use pillar_crypto::types::StdByteArray;
use serde::{Deserialize, Serialize};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use crate::Config;


#[derive(Clone)]
struct AppState {
    node: Node,
    config: Config
}

#[derive(Serialize, Deserialize)]
struct TransactionPost {
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

#[derive(Serialize, Deserialize)]
struct StatusResponse {
    success: bool,
    message: String
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    TransactionPost(TransactionPost)
}


/// Handle a transaction post request from the client
/// - Validates the request
/// - Submits the transaction to the node
/// - Sends a response back to the client
/// - If `register_completion_callback` is true, waits for the transaction to be included in
/// a block and sends a completion message back to the client
async fn handle_transaction_post(websocket: &mut WebSocket, request: TransactionPost, state: &mut AppState){
    tracing::info!("Handling transaction post");

    let result = submit_transaction(
        &mut state.node,
        &mut state.config.wallet,
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

/// Handle a single websocket connection
/// - Receives messages from the client
/// - Sends responses back to the client
/// - Closes the connection when done
async fn handle_connection(mut socket: WebSocket, headers: HeaderMap, mut state: AppState){
    tracing::info!("New WebSocket connection from {:?}", headers.get("sec-websocket-key"));
    let result: Result<(), String> = match socket.recv().await {
        None => {
            tracing::info!("WebSocket connection closed by client");
            Ok(())
        },
        Some(Err(e)) => {
            tracing::error!("WebSocket error: {:?}", e);
            Err("WebSocket error".into())
        }
        Some(Ok(msg)) => {
            tracing::debug!("Received WebSocket message: {:?}", msg);
            match serde_json::from_str::<ClientMessage>(msg.to_text().unwrap_or("")) {
                Ok(ClientMessage::TransactionPost(tx)) => {
                    handle_transaction_post(&mut socket, tx, &mut state).await;
                    Ok(())
                }
                Err(e) => {
                    tracing::debug!("Failed to parse WebSocket message: {:?}", e);
                    Err("Invalid message format".into())
                }
            }
        },
    };
    if let Err(e) = result {
        let response = StatusResponse {
            success: false,
            message: e,
        };
        socket.send(Message::Text(serde_json::to_string(&response).unwrap().into())).await.unwrap();
    }
    tracing::info!("Ending WebSocket connection from {:?}", headers.get("sec-websocket-key"));
}

async fn ws_route(ws: WebSocketUpgrade, headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_connection(socket, headers, state))
}

pub async fn launch_node(config: Config) {
    let mut node = Node::new(
        config.wallet.address,
        config.wallet.get_private_key(),
        config.ip_address,
        pillar_core::PROTOCOL_PORT,
        config.wkps.clone(),
        None,
        None
    );

    node.serve().await;

    let state = AppState {
        node,
        config
    };
    let app = Router::new()
        .route("/ws", get(ws_route))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();

    tracing::info!("Listening on ws://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}