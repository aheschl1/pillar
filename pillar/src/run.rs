use std::sync::Arc;

use axum::{extract::State, routing::post, Json, Router};
use flume::Receiver;
use pillar_core::{accounting::wallet::Wallet, nodes::node::Node, primitives::{block::BlockHeader, errors::QueryError, transaction::Transaction}, protocol::{peers::discover_peers, transactions::submit_transaction}};
use pillar_crypto::{signing::SigFunction, types::StdByteArray};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::Config;

pub const REFRESH_EVERY_SECS: u64 = 60;


#[derive(Clone)]
struct AppState {
    node: Node,
    wallet: Wallet,
}

#[derive(Serialize, Deserialize)]
struct TransactionPost{
    receiver: StdByteArray,
    amount: u64,
    register_completion_callback: bool
}

#[derive(Serialize, Deserialize)]
struct TransactionResponse {
    success: bool,
    message: String,
    transaction_hash: Option<StdByteArray>,
}

async fn post_transaction(State(mut state): State<AppState>, Json(request): Json<TransactionPost>) -> Json<TransactionResponse> {
    // Handle the transaction submission
    let result = submit_transaction(
        &mut state.node,
        &mut state.wallet,
        request.receiver,
        request.amount,
        request.register_completion_callback,
        None
    ).await;
    match result {
        Ok(tx) => Json(TransactionResponse {
            success: true,
            message: "Transaction submitted successfully".to_string(),
            transaction_hash: Some(tx.1.hash)
        }),
        Err(e) => Json(TransactionResponse {
            success: false,
            message: format!("Failed to submit transaction: {:?}", e),
            transaction_hash: None
        })
    }
}

pub async fn launch_node(config: &Config) {
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
        wallet: config.wallet.clone(),
    };
    let app = Router::new()
        .route("/transactions", post(post_transaction))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();

    axum::serve(listener, app).await.unwrap();    
}