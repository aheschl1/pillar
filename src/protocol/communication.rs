use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use tokio::time::{timeout, Duration};

use crate::{
    crypto::hashing::{DefaultHash, HashFunction, Hashable},
    nodes::{
        messages::{Message, Versions, get_declaration_length},
        node::{Broadcaster, Node},
    },
};

/// Background process that consumes mined blocks, and transactions which must be forwarded
pub async fn broadcast_knowledge(node: Node) -> Result<(), std::io::Error> {
    let mut hasher = DefaultHash::new();
    loop {
        // send a message to all peers
        if let Some(pool) = &node.miner_pool {
            // broadcast out of mining pool
            // while pool.ready_block_count() > 0 {
            //     node.broadcast(&Message::BlockTransmission(pool.pop_ready_block().unwrap()))
            //     .await?;
            // }
            while pool.proposed_block_count() > 0 {
                let m = Message::BlockTransmission(
                    pool.pop_block_preposition().await.unwrap(),
                );
                let hash = m.hash(&mut hasher).unwrap();
                let mut broadcased = node.inner.broadcasted_already.lock().await;
                // do not broadcast if already broadcasted
                if broadcased.contains(&hash) {
                    continue;
                }
                // add the message to the broadcasted list
                broadcased.insert(hash);
                // broadcast the message
                node.broadcast(&m).await?;
            }
        }
        let mut i = 0;
        let mut broadcasted_already = node.inner.broadcasted_already.lock().await;
        while i < 10 && !node.inner.broadcast_receiver.is_empty() {
            // receive the transaction from the sender
            let message = node.inner.broadcast_receiver.recv().unwrap();
            let hash = message.hash(&mut hasher).unwrap();
            // do not broadcast if already broadcasted
            if broadcasted_already.contains(&hash) {
                continue;
            }
            node.broadcast(&message).await?;
            broadcasted_already.insert(hash);
            // add the message to the broadcasted list
            i += 1; // We want to make sure we check back at the mining pool
        }
    }
}

/// This function serves as a loop that accepts incoming requests, and handles the main protocol
/// For each connection, listens for a peer declaration. Then, it adds this peer to the peer list if not alteady there
/// After handling the peer, it reads for the actual message. Then, it calls off to the serve_message function.
/// The response from serve_message is finally returned back to the peer
pub async fn serve_peers(node: Node) {
    let listener = TcpListener::bind(format!("{}:{}", node.ip_address, node.port))
        .await
        .unwrap();
    loop {
        // handle connection
        let (mut stream, _) = listener.accept().await.unwrap();
        // spawn a new thread to handle the connection
        let mut self_clone = node.clone();
        tokio::spawn(async move {
            // first read the peer declaration
            let mut buffer = [0; get_declaration_length(Versions::V1V4) as usize];
            let result = timeout(Duration::from_secs(1), stream.read_exact(&mut buffer)).await;
            let status = match &result {
                Ok(Ok(_)) => 0,
                Ok(Err(_)) => 1,
                Err(_) => 2
            };
            let n: usize = if status == 0{
                result.unwrap().unwrap()
            } else {
                // halt communication
                send_error_message(
                    &mut stream,
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        if status == 1 {"Invalid peer declaration"} else {"Declaration timeout."},
                    ),
                ).await;
                return;
            };
            let declaration: Result<Message, Box<bincode::ErrorKind>> = bincode::deserialize(&buffer[..n]);
            if declaration.is_err() {
                // halt communication
                send_error_message(
                    &mut stream,
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Invalid peer delaration",
                    ),
                ).await;
                return;
            }
            let declaration = declaration.unwrap();
            let message_length;
            let declaring_peer = match declaration {
                Message::Declaration(peer, n) => {
                    message_length = n;
                    // add the peer to the list if and only if it is not already in the list
                    self_clone.maybe_update_peer(peer.clone()).await.unwrap();
                    // send a response
                    peer
                }
                _ => {
                    send_error_message(
                        &mut stream,
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Expected peer delaration",
                        ),
                    )
                    .await;
                    return;
                }
            };
            // read actual the message
            let mut buffer = vec![0; message_length as usize];
            let _ = stream.read_exact(&mut buffer).await.unwrap();
            let message: Result<Message, Box<bincode::ErrorKind>> = bincode::deserialize(&buffer);
            if message.is_err() {
                // halt
                send_error_message(&mut stream, message.unwrap_err()).await;
                return;
            }
            let message = message.unwrap();
            let response = self_clone.serve_request(&message, declaring_peer).await;
            match response {
                Err(e) => send_error_message(&mut stream, e).await,
                Ok(message) => {
                    let nbytes = bincode::serialized_size(&message).unwrap() as u32;
                    // write the size of the message as 4 bytes - 4 bytes because we are using u32
                    stream.write_all(&nbytes.to_le_bytes()[..4]).await.unwrap();
                    stream
                        .write_all(&bincode::serialize(&message).unwrap())
                        .await
                        .unwrap()
                }
            };
        });
    }
}

/// sends an error response when given a string description
async fn send_error_message(stream: &mut TcpStream, e: impl std::error::Error) {
    // writye message size
    let nbytes = bincode::serialized_size(&Message::Error(e.to_string())).unwrap() as u32;
    // write the size of the message as 4 bytes - 4 bytes because we are using u32
    stream.write_all(&nbytes.to_le_bytes()[..4]).await.unwrap();
    // write the error message to the stream
    stream
        .write_all(&bincode::serialize(&Message::Error(e.to_string())).unwrap())
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;
    use crate::{
        crypto::hashing::DefaultHash, nodes::{
            messages::Message,
            node::Node,
            peer::Peer,
        }, primitives::transaction::Transaction
    };
    use core::{panic, time};
    use std::net::{IpAddr, Ipv4Addr};
    use std::str::FromStr;

    #[tokio::test]
    async fn test_peer_declaration() {
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8084;
        let node = Node::new([1; 32], [2; 32], ip_address, port, vec![], None, None);

        tokio::spawn(serve_peers(node.clone()));

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let mut stream = TcpStream::connect(format!("{}:{}", ip_address, port))
            .await
            .unwrap();

        let peer = Peer {
            public_key: [3; 32],
            ip_address,
            port: 8085,
        };

        let declaration = Message::Declaration(peer.clone(), 0);
        let serialized = bincode::serialize(&declaration).unwrap();
        stream.write_all(&serialized).await.unwrap();

        // Verify the peer was added
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // Allow time for processing
        let peers = node.inner.peers.lock().await;
        assert!(peers.contains_key(&peer.public_key));
    }

    #[tokio::test]
    async fn test_message_broadcast() {
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8080;
        let node = Node::new([1; 32], [2; 32], ip_address, port, vec![], None, None);

        tokio::spawn(serve_peers(node.clone()));
        tokio::spawn(broadcast_knowledge(node.clone()));

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let mut stream = TcpStream::connect(format!("{}:{}", ip_address, port))
            .await
            .unwrap();

        let peer = Peer {
            public_key: [3; 32],
            ip_address,
            port: 8081,
        };

        // listen as peer to hear from server
        let listener = TcpListener::bind(format!("{}:{}", ip_address, 8081))
            .await
            .unwrap();

        let t = Transaction::new([0; 32], [0; 32], 0, 0, 0, &mut DefaultHash::new());
        let message = Message::TransactionBroadcast(t);
        let serialized_message = bincode::serialize(&message).unwrap();

        let declaration = Message::Declaration(peer.clone(), serialized_message.len() as u32);
        let serialized = bincode::serialize(&declaration).unwrap();
        stream.write_all(&serialized).await.unwrap();

        stream.write_all(&serialized_message).await.unwrap();

        // Verify the message was broadcasted back

        let (mut peer_stream, _) = listener.accept().await.unwrap();
        
        // receive peer declaration from node

        let mut b = [0; get_declaration_length(Versions::V1V4) as usize];
        let _ = peer_stream.read_exact(&mut b).await.unwrap();
        let declaration: Message = bincode::deserialize(&b).unwrap();
        match declaration {
            Message::Declaration(_, size) => {
                assert_eq!(size, serialized_message.len() as u32);
            }
            _ => panic!("Expected a Declaration message"),
        }

        let mut buffer = vec![0; serialized_message.len() as usize];
        let n = peer_stream.read_exact(&mut buffer).await.unwrap();
        let message: Message = bincode::deserialize(&buffer[..n]).unwrap();
        match message {
            Message::TransactionBroadcast(_) => {},
            _ => panic!("Expected a TransactionRequest message"),
        }
        // respond with ack
        let response = Message::TransactionAck;
        // send bytes then message
        let response_serialized = bincode::serialize(&response).unwrap();
        let nbytes = response_serialized.len() as u32;
        peer_stream.write_all(&nbytes.to_le_bytes()).await.unwrap();
        peer_stream.write_all(&response_serialized).await.unwrap();
        
        let broadcasted = node.inner.broadcasted_already.lock().await;
        let mut hasher = DefaultHash::new();
        let message_hash = message.hash(&mut hasher).unwrap();
        assert!(broadcasted.contains(&message_hash));
    }

    #[tokio::test]
    async fn test_error_response() {
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8090;
        let node = Node::new([1; 32], [2; 32], ip_address, port, vec![], None, None);

        tokio::spawn(serve_peers(node));

        // sleep to allow the server to start
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let mut stream = TcpStream::connect(format!("{}:{}", ip_address, port))
            .await
            .unwrap();

        // let invalid_message = vec![0; 10]; // Invalid message
        let message = Message::Ping;
        let serialized_message = bincode::serialize(&message).unwrap();
        stream.write_all(&serialized_message).await.unwrap();

        let mut b = [0; 4];
        let _ = stream.read_exact(&mut b).await;
        let size = u32::from_le_bytes(b);

        let mut buffer = vec![0; size as usize];
        let n = stream.read_exact(&mut buffer).await.unwrap();
        let response: Message = bincode::deserialize(&buffer[..n]).unwrap();

        match response {
            Message::Error(_) => {},
            _ => panic!("Expected an error message"),
        }
    }

}
