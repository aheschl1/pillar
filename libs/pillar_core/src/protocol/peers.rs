use std::collections::HashSet;

use crate::{nodes::{node::Node, peer::Peer}, primitives::messages::Message};

/// Find new peers by queerying the existing peers
/// and adding them to the list of peers
///
/// Args:
/// node: &mut Node - The node to discover peers for
pub async fn discover_peers(node: &mut Node) -> Result<(), std::io::Error> {
    let mut existing_peers = node
        .inner
        .peers
        .lock()
        .await
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    let mut new_peers: Vec<Peer> = vec![];
    // send a message to the peers
    for (_, peer) in node.inner.peers.lock().await.iter_mut() {
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

#[cfg(test)]
mod tests {
    use pillar_crypto::serialization::PillarSerialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::nodes::{node::Node, peer::Peer};
    use crate::primitives::messages::{get_declaration_length, Versions};
    use std::net::{IpAddr, Ipv4Addr};
    use std::str::FromStr;

    #[tokio::test]
    async fn test_discover_peers_adds_new_peers() {
        let existing_peer = Peer {
            public_key: [3; 32],
            ip_address: IpAddr::V4(Ipv4Addr::from_str("127.0.0.2").unwrap()),
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
            None,
            None,
        );
 
        // Mock new peer to be discovered
        let new_peer = Peer {
            public_key: [4; 32],
            ip_address: IpAddr::V4(Ipv4Addr::from_str("127.0.0.3").unwrap()),
            port: 8082,
        };
        let new_peer2 = Peer {
            public_key: [5; 32],
            ip_address: IpAddr::V4(Ipv4Addr::from_str("127.0.0.3").unwrap()),
            port: 8082,
        };

        // Mock peer response
        let mock_response = Message::PeerResponse(vec![new_peer.clone(), new_peer2.clone()]);

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // expect a peer declaration 
            let mut buffer = [0; get_declaration_length(Versions::V1V4) as usize];
            stream.read_exact(&mut buffer).await.unwrap();
            let message: Message = PillarSerialize::deserialize_pillar(&buffer).unwrap();
            let expected_request = Message::PeerRequest;
            let serialized = expected_request.serialize_pillar().unwrap();
            match message {
                Message::Declaration(peer, size) => {
                    assert_eq!(peer.public_key, [1; 32]);
                    assert_eq!(size, serialized.len() as u32);
                }
                _ => panic!("Expected a declaration message"),
            }
            let mut buffer = vec![0; serialized.len() as usize];
            stream.read_exact(&mut buffer).await.unwrap();
            let message: Message = PillarSerialize::deserialize_pillar(&buffer).unwrap();
            match message {
                Message::PeerRequest=>{},
                _ => panic!("Expected a peer request message"),
            }
            // reply with a peer
            // first n bytes
            let serialized = mock_response.serialize_pillar().unwrap();
            let size = serialized.len() as usize;
            stream.write_all(&size.to_le_bytes()[0..4]).await.unwrap();
            let _ = stream.write_all(&serialized).await;
            // done
        });
        // Discover peers
        discover_peers(&mut node).await.unwrap();
        // Verify new peer was added
        let peers = node.inner.peers.lock().await;
        assert!(peers.contains_key(&new_peer.public_key));
        assert!(peers.contains_key(&new_peer2.public_key));
        assert_eq!(peers.len(), 3); // Existing + new peer
    }
}
