use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}};

use crate::nodes::{messages::{get_declaration_length, Message, Versions}, node::{Broadcaster, Node}};

/// Background process that consumes mined blocks, and transactions which must be forwarded
pub async fn broadcast_knowledge(node: Node) -> Result<(), std::io::Error> {
    loop {
        // send a message to all peers
        if let Some(pool) = &node.miner_pool { // broadcast out of mining pool
            while pool.block_count() > 0 {
                node.broadcast(&Message::BlockTransmission(pool.pop_block().unwrap()))
                .await?;
            }
        }
        let mut i = 0;
        while i < 10 && !node.broadcast_receiver.is_empty() {
            // receive the transaction from the sender
            let message = node.broadcast_receiver.recv().unwrap();
            node.broadcast(&message).await?;
            i += 1; // We want to make sure we check back at the mineing pool
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
            let n = stream.read_exact(&mut buffer).await.unwrap();
            // deserialize with bincode
            let declaration: Result<Message, Box<bincode::ErrorKind>> =
            bincode::deserialize(&buffer[..n]);
            if declaration.is_err() {
                // halt communication
                return;
            }
            let declaration = declaration.unwrap();
            let message_length;
            match declaration {
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
            let mut buffer = Vec::with_capacity(message_length as usize);
            let n = stream.read_exact(&mut buffer).await.unwrap();
            let message: Result<Message, Box<bincode::ErrorKind>> =bincode::deserialize(&buffer[..n]);
            if message.is_err() {
                // halt
                return;
            }
            let message = message.unwrap();
            let response = self_clone.serve_request(&message).await;
            match response {
                Err(e) => send_error_message(&mut stream, e).await,
                Ok(message) => {
                    let nbytes = bincode::serialized_size(&message).unwrap() as u32;
                    // write the size of the message as 4 bytes - 4 bytes because we are using u32
                    stream.write(&nbytes.to_le_bytes()[..4]).await.unwrap();
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
    stream.write(&nbytes.to_le_bytes()[..4]).await.unwrap();
    // write the error message to the stream
    stream
        .write_all(&bincode::serialize(&Message::Error(e.to_string())).unwrap())
        .await
        .unwrap();
}