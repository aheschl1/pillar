use axum::{extract::{Path, State}, http::HeaderMap, response::IntoResponse, routing::{get, post}, Json, Router};
use pillar_core::{nodes::{node::Node, peer::Peer}, protocol::peers::discover_peer};
use pillar_crypto::types::StdByteArray;
use serde::{Deserialize, Serialize};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use crate::{handles::{handle_transaction_post, TransactionPost}, Config};


#[derive(Clone)]
struct AppState {
    node: Node,
    config: Config
}

#[derive(Serialize, Deserialize)]
struct StatusResponse {
    success: bool,
    message: String
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    TransactionPost(TransactionPost),
    PeerPost(PeerPost)
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
                    handle_transaction_post(&mut socket, tx, &mut state.node, &mut state.config.wallet).await;
                    Ok(())
                }
                Err(e) => {
                    tracing::debug!("Failed to parse WebSocket message: {:?}", e);
                    Err("Invalid message format".into())
                },
                _ => Err("Unexpected message at websocket endpoint".into())
            }
        }
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

#[derive(Serialize, Deserialize)]
pub(crate) struct PeerPost {
    ip_address: String,
    port: u16,
}

/// Handle HTTP post to add a new peer
/// Not upgraded to WebSocket
/// 
/// # Arguments
/// * `State(state)`: The application state containing the node
/// * `Json(request)`: The peer post request containing the peer details
/// * `Path(public_key)`: The public key of the peer to add, in hex format
async fn handle_peer_post(
    public_key: Option<Path<String>>,
    State(mut state): State<AppState>,
    Json(request): Json<PeerPost>,
) -> impl IntoResponse {
    let ipaddr = match request.ip_address.parse::<std::net::IpAddr>() {
        Ok(ip) => ip,
        Err(_) => {
            let response = StatusResponse {
                success: false,
                message: "Invalid IP address".to_string(),
            };
            return Json(response);
        }
    };

    let public_key = if public_key.is_none() {
        let peer = discover_peer(&mut state.node, ipaddr, request.port).await;
        if peer.is_err() {
            let response = StatusResponse {
                success: false,
                message: format!("Failed to discover peer at {}:{}", request.ip_address, request.port),
            };
            return Json(response);
        }
        let peer = peer.unwrap();
        peer.public_key
    } else {
        let publickey_array = match hex::decode(public_key.unwrap().as_str()) {
            Ok(pk) => pk,
            Err(_) => {
                let response = StatusResponse {
                    success: false,
                    message: "Invalid public key".to_string(),
                };
                return Json(response);
            }
        }.as_slice()
        .try_into();
        if publickey_array.is_err() {
            let response = StatusResponse {
                success: false,
                message: "Public key must be 32 bytes".to_string(),
            };
            return Json(response);
        }
        publickey_array.unwrap()
    };

    tracing::info!("Handling peer post");

    let result = state.node.maybe_update_peer(Peer::new(public_key, ipaddr, request.port)).await;
    let response = if let Err(e) = result {
        StatusResponse {
            success: false,
            message: format!("Failed to add peer: {:?}", e),
        }
    }else{
        StatusResponse {
            success: true,
            message: format!("Peer added: {}:{}", request.ip_address, request.port),
        }
    };
    Json(response)
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
        .route("/peer/:public_key", post(handle_peer_post)) // allow with public key
        .route("/peer", post(handle_peer_post)) // allow without public key
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();

    tracing::info!("Listening on ws://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}