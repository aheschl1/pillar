use pillar_crypto::{hashing::{DefaultHash, Hashable}};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use tokio::time::{timeout, Duration};
use tracing::instrument;

use crate::{
    nodes::node::{Broadcaster, Node}, primitives::messages::Message, protocol::serialization::{package_standard_message, read_standard_message}
};

/// Background process that consumes mined blocks, and transactions which must be forwarded
pub async fn broadcast_knowledge(node: Node, stop_signal: Option<flume::Receiver<()>>) -> Result<(), std::io::Error> {
    let mut hasher = DefaultHash::new();
    loop {
        // send a message to all peers
        if let Some(signal) = &stop_signal {
            if signal.try_recv().is_ok() {
                return Ok(());
            }
        }
        if let Some(pool) = &node.miner_pool {
            while let Some(proposed_block) = pool.pop_block_proposition(){
                let m: Message = Message::BlockTransmission(proposed_block);
                let hash = m.hash(&mut hasher).unwrap();
                let mut broadcased = node.inner.broadcasted_already.write().await;
                // do not broadcast if already broadcasted
                if broadcased.contains(&hash) {
                    continue;
                }
                // add the message to the broadcasted list
                broadcased.insert(hash);
                // drop(broadcased);
                // broadcast the message
                node.broadcast(&m).await?;
            }
        }
        let mut i = 0;
        while i < 10 && let Some(broadcast) = node.inner.broadcast_queue.dequeue() {
            // receive the transaction from the sender
            let hash = broadcast.hash(&mut hasher).unwrap();
            // do not broadcast if already broadcasted
            {
                let mut broadcasted_already = node.inner.broadcasted_already.write().await;
                if broadcasted_already.contains(&hash) {
                    continue;
                }
                broadcasted_already.insert(hash);
            }
            node.broadcast(&broadcast).await?;
            // add the message to the broadcasted list
            i += 1; // We want to make sure we check back at the mining pool
        }
        // this is a hack to simply yield to the runtime
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

/// This function serves as a loop that accepts incoming requests, and handles the main protocol
/// For each connection, listens for a peer declaration. Then, it adds this peer to the peer list if not alteady there
/// After handling the peer, it reads for the actual message. Then, it calls off to the serve_message function.
/// The response from serve_message is finally returned back to the peer
#[instrument(skip_all, fields(ip = %node.ip_address, port = node.port))]
pub async fn serve_peers(node: Node, stop_signal: Option<flume::Receiver<()>>) {
    let listener = TcpListener::bind(format!("{}:{}", node.ip_address, node.port))
        .await
        .unwrap();
    loop {
        // handle connection
        let mut stream = match timeout(tokio::time::Duration::from_secs(3),listener.accept()).await {
            Ok(Ok((stream, _))) => stream,
            Ok(Err(e)) => {
                tracing::error!("Error accepting connection: {}", e);
                continue;
            },
            Err(_) => {
                // check if we should stop
                if let Some(signal) = &stop_signal {
                    if signal.try_recv().is_ok() {
                        break;
                    }
                }
                continue; // timeout, try again
            }     
        };
        // spawn a new thread to handle the connection
        let mut self_clone = node.clone();
        tokio::spawn(async move {
            // first read the peer declaration
            let result = timeout(Duration::from_secs(1), read_standard_message(&mut stream)).await;
            let status = match &result {
                Ok(Ok(_)) => 0,
                Ok(Err(_)) => 1,
                Err(_) => 2
            };
            if status != 0 {
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
            let declaration = result.unwrap().unwrap();
            let declaring_peer = match declaration {
                Message::Declaration(peer) => {
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
            let message: Result<Message, std::io::Error> = read_standard_message(&mut stream).await;
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
                    let bytes = package_standard_message(&message).unwrap();
                    // write the size of the message as 4 bytes - 4 bytes because we are using u32
                    stream
                        .write_all(&bytes)
                        .await
                        .unwrap();
                    tracing::debug!("Sent {} bytes to peer", bytes.len());
                }
            };
        });
    }
}

/// sends an error response when given a string description
#[instrument(skip_all, fields(message=?e, text=e.to_string()))]
async fn send_error_message(stream: &mut TcpStream, e: impl std::error::Error) {
    // write message size
    let serialized = package_standard_message(&Message::Error(e.to_string())).unwrap();
    stream
        .write_all(&serialized)
        .await
        .unwrap();
    tracing::debug!("Sent {} bytes to peer", serialized.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;
    use crate::nodes::peer::Peer;
    use crate::protocol::serialization::{package_standard_message, PillarSerialize};
    use crate::{
        primitives::transaction::Transaction
    };
    use core::panic;
    use std::net::{IpAddr, Ipv4Addr};
    use std::str::FromStr;

    #[tokio::test]
    async fn test_peer_declaration() {
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8084;
        let node = Node::new([1; 32], [2; 32], ip_address, port, vec![], None, None);

        tokio::spawn(serve_peers(node.clone(), None));

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let mut stream = TcpStream::connect(format!("{ip_address}:{port}"))
            .await
            .unwrap();

        let peer = Peer {
            public_key: [3; 32],
            ip_address,
            port: 8085,
        };

        let declaration = Message::Declaration(peer.clone());
        let serialized = package_standard_message(&declaration).unwrap();
        stream.write_all(&serialized).await.unwrap();

        // Verify the peer was added
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // Allow time for processing
        let peers = node.inner.peers.read().await;
        assert!(peers.contains_key(&peer.public_key));
    }

    #[tokio::test]
    async fn test_message_broadcast() {
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8080;
        let node = Node::new([1; 32], [2; 32], ip_address, port, vec![], None, None);

        tokio::spawn(serve_peers(node.clone(), None));
        tokio::spawn(broadcast_knowledge(node.clone(), None));

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let mut stream = TcpStream::connect(format!("{ip_address}:{port}"))
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


        let declaration = Message::Declaration(peer.clone());
        stream.write_all(&package_standard_message(&declaration).unwrap()).await.unwrap();

        let t = Transaction::new([0; 32], [0; 32], 0, 0, 0, &mut DefaultHash::new());
        let message = Message::TransactionBroadcast(t);
        stream.write_all(&package_standard_message(&message).unwrap()).await.unwrap();

        // Verify the message was broadcasted back

        let (mut peer_stream, _) = listener.accept().await.unwrap();
        
        // receive peer declaration from node
        let declaration: Message = read_standard_message(&mut peer_stream).await.unwrap();
        match declaration {
            Message::Declaration(_) => {},
            _ => panic!("Expected a Declaration message"),
        }

        let message: Message = read_standard_message(&mut peer_stream).await.unwrap();
        match message {
            Message::TransactionBroadcast(_) => {},
            _ => panic!("Expected a TransactionRequest message"),
        }
        // respond with ack
        let response = Message::TransactionAck;
        // send bytes then message
        let response_serialized = package_standard_message(&response).unwrap();
        peer_stream.write_all(&response_serialized).await.unwrap();
        
        let broadcasted = node.inner.broadcasted_already.read().await;
        let mut hasher = DefaultHash::new();
        let message_hash = message.hash(&mut hasher).unwrap();
        assert!(broadcasted.contains(&message_hash));
    }

    #[tokio::test]
    async fn test_error_response() {
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap());
        let port = 8090;
        let node = Node::new([1; 32], [2; 32], ip_address, port, vec![], None, None);

        tokio::spawn(serve_peers(node,None));

        // sleep to allow the server to start
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let mut stream = TcpStream::connect(format!("{ip_address}:{port}"))
            .await
            .unwrap();

        let invalid_message = vec![0; 22]; // Invalid message
        stream.write_all(&invalid_message).await.unwrap();

        let response: Message = read_standard_message(&mut stream).await.unwrap();

        match response {
            Message::Error(_) => {},
            _ => panic!("Expected an error message"),
        }

        // send valid ping, but raw serialized
        stream = TcpStream::connect(format!("{ip_address}:{port}"))
            .await
            .unwrap();
        let ping = Message::Ping;
        let serialized = ping.serialize_pillar().unwrap();
        stream.write_all(&serialized).await.unwrap();
        // expect error

        let response: Message = read_standard_message(&mut stream).await.unwrap();

        match response {
            Message::Error(_) => {},
            _ => panic!("Expected an error message"),
        }
    }

    #[tokio::test]
    async fn test_timeout_broadcast(){
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.9").unwrap());
        let port = 8091;
        let node = Node::new([1; 32], [2; 32], ip_address, port, vec![], None, None);

        let (sender, stop_signal) = flume::bounded(1); // Create a stop signal receiver
        let handle = tokio::spawn(broadcast_knowledge(node.clone(), Some(stop_signal.clone())));

        // Sleep for a short duration to allow the broadcast loop to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Send a stop signal to the broadcast loop
        let now = std::time::Instant::now();
        sender.send(()).unwrap();
        // Wait for a short duration to ensure the broadcast loop has time to process the stop signal
        let _ = handle.await.unwrap();
        let elapsed = now.elapsed();
        // Check that the broadcast loop stopped within a reasonable time
        assert!(elapsed.as_millis() < 500, "Broadcast loop did not stop in time");
    }

    #[tokio::test]
    async fn test_timeout_serve(){
        let ip_address = IpAddr::V4(Ipv4Addr::from_str("127.0.0.9").unwrap());
        let port = 8091;
        let node = Node::new([1; 32], [2; 32], ip_address, port, vec![], None, None);

        let (sender, stop_signal) = flume::bounded(1); // Create a stop signal receiver
        let handle = tokio::spawn(serve_peers(node.clone(), Some(stop_signal.clone())));
        // Sleep for a short duration to allow the server to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        // Send a stop signal to the server
        let now = std::time::Instant::now();
        sender.send(()).unwrap();
        // Wait for a short duration to ensure the server has time to process the stop signal
        handle.await.unwrap();
        let elapsed = now.elapsed();
        // Check that the server stopped within a reasonable time
        assert!(elapsed.as_secs() < 3, "Server did not stop in time");
    }

}
