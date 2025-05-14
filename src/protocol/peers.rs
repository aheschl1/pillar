use std::collections::{HashMap, HashSet};

use crate::nodes::{messages::Message, node::Node, peer::Peer};

/// Find new peers by queerying the existing peers
/// and adding them to the list of peers
///
/// Args:
/// node: &mut Node - The node to discover peers for
pub async fn discover_peers(node: &mut Node) -> Result<(), std::io::Error> {
    let mut existing_peers = node
        .peers
        .lock()
        .await
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    let mut new_peers: HashMap<[u8; 32], Peer> = HashMap::new();
    // send a message to the peers
    for (_, peer) in node.peers.lock().await.iter_mut() {
        let peers = peer
            .communicate(&Message::PeerRequest, &node.clone().into())
            .await?;
        match peers {
            Message::PeerResponse(peers) => {
                for peer in peers {
                    // check if the peer is already in the list
                    if !existing_peers.contains(&peer.public_key) {
                        // add the peer to the list
                        existing_peers.insert(peer.public_key);
                        new_peers.insert(peer.public_key, peer);
                    }
                }
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Invalid message received",
                ));
            }
        }
    }
    // extend the peers list with the new peers
    node.peers.lock().await.extend(new_peers);
    Ok(())
}
