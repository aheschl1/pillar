use std::sync::{Arc};

use axum::{extract::{Path, Query, State}, http::HeaderMap, response::IntoResponse, routing::{get, post}, Json, Router};
use pillar_core::{accounting::{account, wallet::{self, Wallet}}, nodes::{miner::Miner, node::{Node, StartupModes}, peer::Peer}, primitives::pool::MinerPool, protocol::peers::discover_peer, reputation::history::NodeHistory};
use pillar_crypto::types::StdByteArray;
use serde::{Deserialize, Serialize};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use tokio::sync::RwLock;
use crate::{log_stream::ws_logs, ws_handles::{handle_transaction_post, TransactionPost}, Config};
use tower_http::cors::{CorsLayer, Any};


#[derive(Clone)]
struct AppState {
    node: Node,
    config: Arc<RwLock<Config>>
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
                    let wallet = &mut state.config.write().await.wallet;
                    handle_transaction_post(
                        &mut socket, 
                        tx,
                        &mut state.node, 
                        wallet
                    ).await;
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

    tracing::info!("Handling peer post for peer {} at {}:{}", hex::encode(public_key), request.ip_address, request.port);

    let result = state.node.maybe_update_peer(Peer::new(public_key, ipaddr, request.port)).await;
    let response = if let Err(e) = result {
        StatusResponse::error(format!("Failed to add peer: {:?}", e))
    }else{
        tracing::info!("Successfully added peer {} at {}:{}", hex::encode(public_key), request.ip_address, request.port);
        StatusResponse::success("success".to_string())
    };
    Json(response)
}

#[derive(Serialize, Deserialize)]
struct PeerResponse {
    public_key: StdByteArray,
    ip_address: String,
    port: u16,
}

async fn handle_peer_get(
    State(state): State<AppState>,
) -> Json<StatusResponse<Vec<PeerResponse>>> {
    let peers = state.node.inner.peers.read().await.clone();
    let peer_responses: Vec<PeerResponse> = peers.into_iter().map(|(_, peer)| PeerResponse {
        public_key: peer.public_key,
        ip_address: peer.ip_address.to_string(),
        port: peer.port,
    }).collect();
    Json(StatusResponse::success(peer_responses))
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

#[derive(Serialize, Deserialize)]
struct NodeInfo {
    public_key: StdByteArray,
    ip_address: String,
    port: u16,
    peer_count: usize,
    state: String
}

async fn handle_node_get(
    State(state): State<AppState>,
) -> Json<StatusResponse<NodeInfo>> {
    let peer_count = state.node.inner.peers.read().await.len();
    let node_state = state.node.inner.state.read().await.clone();
    let response = NodeInfo {
        public_key: state.config.read().await.wallet.address,
        ip_address: state.node.ip_address.to_string(),
        port: state.node.port,
        peer_count,
        state: node_state.into()
    };
    Json(StatusResponse::success(response))
}

#[derive(Serialize, Deserialize)]
struct WalletInfo {
    private_key: String,
    public_key: StdByteArray,
    balance: u64,
    nonce: u64
}

async fn handle_wallet_get(
    State(state): State<AppState>,
) -> Json<StatusResponse<WalletInfo>> {
    let wallet = &state.config.read().await.wallet;

    let chain = state.node
        .lock_chain()
        .await;

    if chain.is_none() {
        return Json(StatusResponse::error("Node has no chain".to_string()));
    }
    let chain = chain.as_ref().unwrap();

    let state_root = chain.get_state_root();
    if let None = state_root {
        return Json(StatusResponse::error("Node has no state root".to_string()));
    }
    let state_root = state_root.unwrap();

    let account = chain
        .state_manager
        .get_account(&wallet.address, state_root);

    if let None = account {
        return Json(StatusResponse::error("Failed to get account data from the chain (have you used it yet?)".to_string()));
    }
    let account = account.unwrap();

    let response = WalletInfo {
        private_key: hex::encode(wallet.get_private_key()),
        public_key: wallet.address,
        balance: account.balance,
        nonce: account.nonce
    };
    Json(StatusResponse::success(response))
}

#[derive(Serialize, Deserialize)]
struct TransactionResponse {
    signature: String,
    sender: StdByteArray,
    receiver: StdByteArray,
    amount: u64,
    timestamp: u64,
    nonce: u64,
    hash: StdByteArray
}

async fn handle_transaction_get(
    Path((block_hash, hash)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Json<StatusResponse<TransactionResponse>> {
    let block_hash_bytes = match hex::decode(&block_hash) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Json(StatusResponse::error("Invalid block hash".to_string()));
        }
    };
    if block_hash_bytes.len() != 32 {
        return Json(StatusResponse::error("Block hash must be 32 bytes".to_string()));
    }
    let block_hash_array: [u8; 32] = block_hash_bytes.as_slice().try_into().unwrap();

    let hash_bytes = match hex::decode(&hash) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Json(StatusResponse::error("Invalid transaction hash".to_string()));
        }
    };
    if hash_bytes.len() != 32 {
        return Json(StatusResponse::error("Transaction hash must be 32 bytes".to_string()));
    }
    let hash_array: [u8; 32] = hash_bytes.as_slice().try_into().unwrap();

    let chain = state.node.lock_chain().await;
    let block = match &*chain {
        Some(chain) => chain.get_block(&block_hash_array),
        None => {
            return Json(StatusResponse::error("Node has no chain".to_string()));
        }
    };
    match block {
        None => {
            return Json(StatusResponse::error("Block not found".to_string()));
        }
        Some(block) => {
            let transaction = block.transactions.iter().find(|tx| tx.hash == hash_array);
            match transaction {
                None => {
                    return Json(StatusResponse::error("Transaction not found in block".to_string()));
                }
                Some(tx) => {
                    let response = TransactionResponse {
                        signature: hex::encode(tx.signature),
                        sender: tx.header.sender,
                        receiver: tx.header.receiver,
                        amount: tx.header.amount,
                        timestamp: tx.header.timestamp,
                        nonce: tx.header.nonce,
                        hash: tx.hash
                    };
                    return Json(StatusResponse::success(response));
                }
            }
        },
    }
}

#[derive(Serialize, Deserialize)]
struct AccountInfo {
    address: StdByteArray,
    balance: u64,
    reputation: f64,
    nonce: u64,
}

#[derive(Serialize, Deserialize)]
struct StateResponse {
    root: StdByteArray,
    accounts: Vec<AccountInfo>,
}

async fn handle_state_get(
    Path(block_hash): Path<String>,
    State(state): State<AppState>,
) -> Json<StatusResponse<StateResponse>> {
    let block_hash_bytes = match hex::decode(&block_hash) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Json(StatusResponse::error("Invalid block hash".to_string()));
        }
    };
    if block_hash_bytes.len() != 32 {
        return Json(StatusResponse::error("Block hash must be 32 bytes".to_string()));
    }
    let block_hash_array: [u8; 32] = block_hash_bytes.as_slice().try_into().unwrap();

    let chain = state.node.lock_chain().await;
    let block = match &*chain {
        Some(chain) => chain.get_block(&block_hash_array),
        None => {
            return Json(StatusResponse::error("Node has no chain".to_string()));
        }
    };
    match block {
        None => {
            return Json(StatusResponse::error("Block not found".to_string()));
        }
        Some(block) => {
            let state_root = match &block.header.completion.as_ref() {
                Some(completion) => completion.state_root,
                None => {
                    return Json(StatusResponse::error("Block is not completed yet".to_string()));
                }
            };
            let time = block.header.timestamp;
            let accounts_map = chain.as_ref().unwrap().state_manager.get_all_accounts(state_root);
            let accounts: Vec<AccountInfo> = accounts_map.iter().map(|account| {
                let history = account.history.as_ref();
                AccountInfo {
                    address: account.address,
                    balance: account.balance,
                    reputation: if let Some(history) = history {
                        history.compute_reputation(time)
                    } else {
                        0.0
                    },
                    nonce: account.nonce,
                }
            }).collect();
            let response = StateResponse {
                root: state_root,
                accounts,
            };
            return Json(StatusResponse::success(response));
        }
    }
}

async fn handle_init_download(
    State(state): State<AppState>,
) -> Json<StatusResponse<()>> {
    tracing::info!("Triggering chain init.");
    state.node.initialize_chain().await;
    Json(StatusResponse { success: true, error: None, body: None })
}

pub async fn launch_node(config: Config, genesis: bool, miner: bool) {
    let mut node = Node::new(
        config.wallet.address,
        config.wallet.get_private_key(),
        config.ip_address,
        pillar_core::PROTOCOL_PORT,
        vec![],
        if genesis { StartupModes::Genesis } else { StartupModes::Empty },
        if miner { Some(MinerPool::new()) } else { None }
    );
    
    for peer in &config.wkps {
        tracing::info!("Configuring well-known peer: {}:{}", peer.ip_address, peer.port);
        // we need to actually discover the peer to get its public key
        let discovered = discover_peer(&mut node, peer.ip_address.into(), peer.port).await;
        match discovered {
            Ok(peer) => {
                node.maybe_update_peer(peer).await.ok();
                tracing::info!("Discovered peer: {}:{}", peer.ip_address, peer.port);
            },
            Err(e) => {
                tracing::error!("Failed to discover peer: {}:{} - {:?}", peer.ip_address, peer.port, e);
            }
        }
    }

    if miner {
        let mut miner = Miner::new(node.clone()).expect("Failed to create miner");
        miner.serve().await;
    }else{
        node.serve().await;
    }


    let api_address = format!("{}:3000", config.ip_address);
    let logs_address = format!("{}:3001", config.ip_address);
    let state = AppState {
        node,
        config: Arc::new(RwLock::new(config))
    };
    let cors = CorsLayer::new()
        .allow_origin(Any)      // allow all origins
        .allow_methods(Any)     // allow all HTTP methods
        .allow_headers(Any);    // allow all headers
    let app_api = Router::new()
        .route("/ws", get(ws_route))
        .route("/peer/{public_key}", post(handle_peer_post)) // allow with public key
        .route("/peer", post(handle_peer_post)) // allow without public key
        .route("/peers", get(handle_peer_get))
        .route("/block/{hash}", get(handle_block_get))
        .route("/transaction/{block_hash}/{hash}", get(handle_transaction_get))
        .route("/state/{block_hash}", get(handle_state_get))
        .route("/blocks", get(handle_block_list))
        .route("/node", get(handle_node_get))
        .route("/wallet", get(handle_wallet_get))
        .route("/init", post(handle_init_download))
        .with_state(state)
        .layer(cors);
    
    let app_logs = Router::new().route("/logs", get(ws_logs));

    let api_listener = tokio::net::TcpListener::bind(api_address.clone()).await.unwrap();
    let logs_listener = tokio::net::TcpListener::bind(logs_address.clone()).await.unwrap();

    tracing::info!("Listening on {}", api_address);
    tracing::info!("Listening for log requests on {}", logs_address);

    let _ = tokio::join!(
        axum::serve(api_listener, app_api),
        axum::serve(logs_listener, app_logs)
    );
}