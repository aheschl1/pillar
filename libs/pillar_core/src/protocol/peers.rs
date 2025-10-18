use std::collections::HashSet;

use crate::{nodes::{node::Node, peer::Peer}, primitives::messages::Message};

/// Find new peers by queerying the existing peers
/// and adding them to the list of peers
///
/// Args:
/// node: &mut Node - The node to discover peers for
pub async fn discover_peers(node: &Node) -> Result<(), std::io::Error> {
    let mut existing_peers = node
        .inner
        .peers
        .read()
        .await
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    let mut new_peers: Vec<Peer> = vec![];
    // send a message to the peers
    for (_, peer) in node.inner.peers.read().await.iter() {
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
                        new_peers.push(peer);
                    }
                }
            }
            _ => {
                return Err(std::io::Error::other("Invalid message received"));
            }
        }
    }
    // extend the peers list with the new peers
    // node.peers.lock().unwrap().extend(new_peers);
    node.maybe_update_peers(new_peers).await;
    Ok(())
}

pub async fn discover_peer(node: &mut Node, ip_address: std::net::IpAddr, port: u16) -> Result<Peer, std::io::Error> {
    // send a message to the peer
    let peer = Peer::new([0; 32], ip_address, port); // dummy public key is replaced by the real one in the response
    let response = peer.communicate(&Message::DiscoveryRequest, &node.clone().into()).await?;
    match response {
        Message::DiscoveryResponse(peer) => {
            Ok(peer)
        }
        _ => Err(std::io::Error::other(format!("Invalid message received: {response:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::nodes::{node::Node, peer::Peer};
    use crate::protocol::serialization::{package_standard_message, read_standard_message};
    use std::net::{IpAddr, Ipv4Addr};
    use std::str::FromStr;

    #[tokio::test]
    async fn test_discover_peers_adds_new_peers() {
        let existing_peer = Peer {
            public_key: [3; 32],
            ip_address: IpAddr::V4(Ipv4Addr::from_str("127.0.0.2").unwrap()).into(),
            port: 8081,
        };
        let listener = tokio::net::TcpListener::bind(format!(
            "{}:{}",
            existing_peer.ip_address, existing_peer.port
        )).await.unwrap();

        let mut node = Node::new(
            [1; 32],
            [2; 32],
            IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap()),
            8080,
            vec![existing_peer],
            false
        );
 
        // Mock new peer to be discovered
        let new_peer = Peer {
            public_key: [4; 32],
            ip_address: IpAddr::V4(Ipv4Addr::from_str("127.0.0.3").unwrap()).into(),
            port: 8082,
        };
        let new_peer2 = Peer {
            public_key: [5; 32],
            ip_address: IpAddr::V4(Ipv4Addr::from_str("127.0.0.3").unwrap()).into(),
            port: 8082,
        };

        // Mock peer response
        let mock_response = Message::PeerResponse(vec![new_peer.clone(), new_peer2.clone()]);

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // expect a peer declaration 
            let message: Message = read_standard_message(&mut stream).await.unwrap();
            match message {
                Message::Declaration(peer) => {
                    assert_eq!(peer.public_key, [1; 32]);
                }
                _ => panic!("Expected a declaration message"),
            }
            let message: Message = read_standard_message(&mut stream).await.unwrap();
            match message {
                Message::PeerRequest => {},
                _ => panic!("Expected a peer request message"),
            }
            // reply with a peer
            // first n bytes
            let serialized = package_standard_message(&mock_response).unwrap();
            let _ = stream.write_all(&serialized).await;
            // done
        });
        // Discover peers
        discover_peers(&mut node).await.unwrap();
        // Verify new peer was added
        let peers = node.inner.peers.read().await;
        assert!(peers.contains_key(&new_peer.public_key));
        assert!(peers.contains_key(&new_peer2.public_key));
        assert_eq!(peers.len(), 3); // Existing + new peer
    }
}
