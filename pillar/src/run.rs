use pillar_core::{accounting::wallet::{self, Wallet}, nodes::node::Node, persistence::database::{Datastore, EmptyDatastore}, protocol::peers::discover_peers};
use pillar_crypto::signing::SigFunction;

use crate::Config;

pub const REFRESH_EVERY_SECS: u64 = 60;

pub async fn peer_refresh(mut node: Node, receiver: flume::Receiver<()>) {
    // loop until we receive a message on the receiver
    while let Err(_) = receiver.try_recv() {
        let result = discover_peers(&mut node).await;
        if let Err(e) = result {
            tracing::error!("Error discovering peers: {}", e);
        }
        tokio::time::sleep(std::time::Duration::from_secs(REFRESH_EVERY_SECS)).await;
    }
}

pub async fn launch_node(config: &Config) {
    let wallet = Wallet::generate_random();
    let mut node = Node::new(
        wallet.address, 
        wallet.get_private_key(), 
        config.ip_address, 
        pillar_core::PROTOCOL_PORT, 
        config.wkps.clone(),
        None,
        None
    );

    let (_shutdown_sender, shutdown_receiver) = flume::unbounded();

    node.serve().await;

    tokio::spawn(peer_refresh(node.clone(), shutdown_receiver));

    tokio::signal::ctrl_c().await.expect("failed to listen for ctrl-c");
}