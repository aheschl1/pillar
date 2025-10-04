use axum::{extract::{Path, Query, State}, http::HeaderMap, response::IntoResponse, routing::{get, post}, Json, Router};
use pillar_core::{nodes::{node::Node, peer::Peer}, protocol::peers::discover_peer};
use pillar_crypto::types::StdByteArray;
use serde::{Deserialize, Serialize};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use crate::{ws_handles::{handle_transaction_post, TransactionPost}, Config};


#[derive(Clone)]
struct AppState {
    node: Node,
    config: Config
}

#[derive(Serialize, Deserialize)]
struct StatusResponse<T: Serialize> {
    success: bool,
    error: Option<String>,
    body: Option<T>,
}

impl<T: Serialize> StatusResponse<T> {
    fn error(message: String) -> Self {
        StatusResponse {
            success: false,
            error: Some(message),
            body: None,
        }
    }

    fn success(body: T) -> Self {
        StatusResponse {
            success: true,
            error: None,
            body: Some(body),
        }
    }
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
async fn handle_ws_connection(mut socket: WebSocket, headers: HeaderMap, mut state: AppState){
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
            error: Some(e),
            body: None::<()>,
        };
        socket.send(Message::Text(serde_json::to_string(&response).unwrap().into())).await.unwrap();
    }
    tracing::info!("Ending WebSocket connection from {:?}", headers.get("sec-websocket-key"));
}

async fn ws_route(ws: WebSocketUpgrade, headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, headers, state))
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
) -> Json<StatusResponse<String>> {
    let ipaddr = match request.ip_address.parse::<std::net::IpAddr>() {
        Ok(ip) => ip,
        Err(_) => {
            return Json(StatusResponse::error("Invalid IP address".to_string()));
        }
    };

    let public_key = if public_key.is_none() {
        let peer = discover_peer(&mut state.node, ipaddr, request.port).await;
        if peer.is_err() {
            return Json(StatusResponse::error(format!("Failed to discover peer at {}:{}", request.ip_address, request.port)));
        }
        let peer = peer.unwrap();
        peer.public_key
    } else {
        let publickey_array = match hex::decode(public_key.unwrap().as_str()) {
            Ok(pk) => pk,
            Err(_) => {
                return Json(StatusResponse::error("Invalid public key".to_string()));
            }
        }.as_slice()
        .try_into();
        if publickey_array.is_err() {
            return Json(StatusResponse::error("Public key must be 32 bytes".to_string()));
        }
        publickey_array.unwrap()
    };

    tracing::info!("Handling peer post");

    let result = state.node.maybe_update_peer(Peer::new(public_key, ipaddr, request.port)).await;
    let response = if let Err(e) = result {
        StatusResponse::error(format!("Failed to add peer: {:?}", e))
    }else{
        StatusResponse::success("success".to_string())
    };
    Json(response)
}

#[derive(Serialize, Deserialize)]
struct HeaderResponse {
    previous: StdByteArray,
    merkle_root: StdByteArray,
    timestamp: u64,
    nonce: u32,
    depth: u64,
    version: u16,
    miner: StdByteArray,
    state_root: StdByteArray,
    difficulty_target: u64,
    stampers: Vec<StdByteArray>
}

#[derive(Serialize, Deserialize)]
struct BlockResponse {
    hash: StdByteArray,
    header: HeaderResponse,
    transaction_hashs: Vec<StdByteArray>
}

async fn handle_block_get(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Json<StatusResponse<BlockResponse>> {
    let hash_bytes = match hex::decode(&hash) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Json(StatusResponse::error("Invalid block hash".to_string()));
        }
    };
    if hash_bytes.len() != 32 {
        return Json(StatusResponse::error("Block hash must be 32 bytes".to_string()));
    }
    let hash_array: [u8; 32] = hash_bytes.as_slice().try_into().unwrap();

    let chain = state.node.lock_chain().await;
    let block = match &*chain {
        Some(chain) => chain.get_block(&hash_array),
        None => {
            return Json(StatusResponse::error("Node has no chain".to_string()));
        }
    };
    match block {
        None => {
            return Json(StatusResponse::error("Block not found".to_string()));
        }
        Some(block) => {
            let header_response = HeaderResponse{
                previous: block.header.previous_hash,
                merkle_root: block.header.merkle_root,
                timestamp: block.header.timestamp,
                nonce: block.header.nonce as u32,
                depth: block.header.depth,
                version: u16::from_le_bytes(block.header.version),
                miner: block.header.completion.as_ref().map_or([0u8; 32], |c| c.miner_address),
                state_root: block.header.completion.as_ref().map_or([0u8; 32], |c| c.state_root),
                difficulty_target: block.header.completion.as_ref().map_or(0, |c| c.difficulty_target),
                stampers: block.header.tail.get_stampers().iter().cloned().collect()
            };
            let response = BlockResponse {
                hash: block.header.completion.as_ref().unwrap().hash,
                header: header_response,
                transaction_hashs: block.transactions.iter().map(|tx| tx.hash).collect()
            };
            Json(StatusResponse::success(response))
        },
    }
}

#[derive(Deserialize)]
struct BlockQuery{
    min_depth: Option<u64>,
    max_depth: Option<u64>,
    limit: Option<usize>
}

/// Handle HTTP get to list blocks
/// Returns a list of block hashes
/// # Arguments
/// * `State(state)`: The application state containing the node
/// * `Query(query)`: The block query parameters
///   - `min_depth`: Minimum depth of blocks to return
///   - `max_depth`: Maximum depth of blocks to return
///   - `limit`: Maximum number of blocks to return
///
/// The returned blocks are ordered from deepest to shallowest, in DFS order
async fn handle_block_list(
    Query(query): Query<BlockQuery>,
    State(state): State<AppState>,
) -> Json<StatusResponse<Vec<StdByteArray>>> {
    let chain = state.node.lock_chain().await;
    let response = match &*chain {
        Some(chain) => {
            let hashes = chain.query_blocks(query.min_depth, query.max_depth, query.limit);
            StatusResponse::success(hashes)
        },
        None => {
            StatusResponse::error("Node has no chain".to_string())
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
        .route("/peer/{public_key}", post(handle_peer_post)) // allow with public key
        .route("/peer", post(handle_peer_post)) // allow without public key
        .route("/block/{hash}", get(handle_block_get))
        .route("/blocks", get(handle_block_list))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();

    tracing::info!("Listening on ws://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}