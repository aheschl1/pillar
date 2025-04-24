use std::{collections::HashSet, net::{IpAddr, Ipv6Addr}, sync::Arc};
use tokio::{net::{TcpListener, TcpStream}, sync::Mutex};

use messages::Message;
use serde::{Serialize, Deserialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{blockchain::chain::Chain, primitives::{pool::TransactionPool, transaction::{self, Transaction}}};

pub mod miner;
pub mod messages;

#[derive(Serialize, Deserialize, Debug)]
pub struct Peer{
    /// The public key of the peer
    pub public_key: [u8; 32],
    /// The IP address of the peer
    pub ip_address: IpAddr,
    /// The port of the peer
    pub port: u16,
}

impl Clone for Peer {
    fn clone(&self) -> Self {
        Peer {
            public_key: self.public_key,
            ip_address: self.ip_address.clone(),
            port: self.port
        }
    }
}

impl Peer{
    /// Create a new peer
    pub fn new(public_key: [u8; 32], ip_address: IpAddr, port: u16) -> Self {
        Peer {
            public_key,
            ip_address,
            port
        }
    }

    /// Send a message to the peer
    /// Initializaes a new connection to the peer
    async fn send_initial(&mut self, message: &Message, initializing_peer: Peer) -> Result<TcpStream, std::io::Error> {
        let mut stream = tokio::net::TcpStream::connect(format!("{}:{}", self.ip_address, self.port)).await?;
        // always send a "peer" object of the initializing node first
        let declaration = Message::Declaration(initializing_peer);
        // serialize with bincode
        stream.write_all(bincode::serialize(&declaration).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?.as_slice()).await?;
        // send the message
        stream.write_all(bincode::serialize(&message).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?.as_slice()).await?;
        Ok(stream)
    }

    /// Get a response from the peer
    /// This function will block until a response is received
    async fn read_response(&self, mut stream: TcpStream) -> Result<Message, std::io::Error> {
        // read the message
        let mut buffer = [0; 2048];
        let n = stream.read(&mut buffer).await?;
        // deserialize with bincode
        let message: Message = bincode::deserialize(&buffer[..n]).map_err(
            |e| std::io::Error::new(std::io::ErrorKind::Other, e)
        )?;
        Ok(message)
    }

    pub async fn communicate(&mut self, message: &Message, initializing_peer: Peer) -> Result<Message, std::io::Error> {
        let stream = self.send_initial(message, initializing_peer).await?;
        let response = self.read_response(stream).await?;
        Ok(response)
    }
}

#[derive(Clone)]
pub struct Node{
    pub public_key: Arc<[u8; 32]>,
    /// The private key of the node
    pub private_key: Arc<[u8; 32]>,
    /// The IP address of the node
    pub ip_address: IpAddr,
    /// The port of the node
    pub port: Arc<u16>,
    // known peers
    pub peers: Arc<Mutex<Vec<Peer>>>,
    // the blockchain
    pub chain: Arc<Mutex<Chain>>,
    /// transactions to be serviced
    pub transaction_pool: Option<Arc<Mutex<TransactionPool>>>
}

impl Node {
    /// Create a new node
    pub fn new(
        public_key: [u8; 32], 
        private_key: [u8; 32], 
        ip_address: IpAddr, 
        port: u16,
        peers: Vec<Peer>,
        chain: Chain,
        transaction_pool: Option<TransactionPool>
    ) -> Self {
        Node {
            public_key: public_key.into(),
            private_key: private_key.into(),
            ip_address: ip_address,
            port: port.into(),
            peers: Mutex::new(peers).into(),
            chain: Mutex::new(chain).into(),
            transaction_pool: transaction_pool.map(|pool| Arc::new(Mutex::new(pool)))
        }
    }

    /// when called, launches a new thread that listens for incoming connections
    pub fn serve(&self){
        // spawn a new thread to handle the connection
        let self_clone = self.clone();
        tokio::spawn(self_clone.serve_peers());
    }

    /// This function serves as a loop that accepts incoming requests, and handles the main protocol
    /// For each connection, listens for a peer declaration. Then, it adds this peer to the peer list if not alteady there
    /// After handling the peer, it reads for the actual message. Then, it calls off to the serve_message function.
    /// The response from serve_message is finally returned back to the peer
    async fn serve_peers(self){
        let listener = TcpListener::bind(format!("{}:{}", self.ip_address, self.port)).await.unwrap();
        loop {
            // handle connection
            let (mut stream, _) = listener.accept().await.unwrap();
            // spawn a new thread to handle the connection
            let mut self_clone = self.clone();
            tokio::spawn(async move {
                // first read the peer declaration
                let mut buffer = [0; 2048];
                let n = stream.read(&mut buffer).await.unwrap();
                // deserialize with bincode
                let declaration: Result<Message, Box<bincode::ErrorKind>> = bincode::deserialize(&buffer[..n]);
                if let Err(_) = declaration{
                    // halt communication
                    return;
                }
                let declaration = declaration.unwrap();
                match declaration {
                    Message::Declaration(peer) => {
                        // add the peer to the list if and only if it is not already in the list
                        self_clone.maybe_update_peers(peer.clone()).await.unwrap();
                        // send a response
                        peer
                    },
                    _ => {
                        self_clone.send_error_message(&mut stream, std::io::Error::new(std::io::ErrorKind::InvalidInput, "Expected peer delaration")).await;
                        return;
                    }
                };
                // read actual the message
                let n = stream.read(&mut buffer).await.unwrap();
                let message: Result<Message, Box<bincode::ErrorKind>> = bincode::deserialize(&buffer[..n]);
                if let Err(_) = message{
                    // halt
                    return;
                }
                let message = message.unwrap();
                let response = self_clone.serve_request(&message).await;
                match response{
                    Err(e) => self_clone.send_error_message(&mut stream, e).await,
                    Ok(message) => stream.write_all(&bincode::serialize(&message).unwrap()).await.unwrap()
                };
            });
        }
    }


    /// Derive the response to a request from a peer
    async fn serve_request(&mut self, message: &Message) -> Result<Message, std::io::Error>{
        match message{
            Message::PeerRequest => {
                // send all peers
                Ok(Message::PeerResponse(self.peers.lock().await.clone()))
            },
            Message::ChainRequest => {
                Ok(Message::ChainResponse(self.chain.lock().await.clone()))
            },
            Message::TransactionRequest(transaction) => {
                // add the transaction to the pool
                match self.transaction_pool{
                    Some(ref pool) => {
                        pool.lock().await.add_transaction(transaction.clone());
                    },
                    None => {}
                }
                Ok(Message::TransactionAck)
            },
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Expected a request"))
        }
    }

    /// sends an error response when given a string description
    async fn send_error_message(&self, stream: &mut TcpStream, e: impl std::error::Error){
        stream.write_all(&bincode::serialize(&Message::Error(e.to_string())).unwrap()).await.unwrap();
    }

    async fn maybe_update_peers(&self, peer: Peer) -> Result<(), std::io::Error> {
        // check if the peer is already in the list
        let mut peers = self.peers.lock().await;
        if !peers.iter().any(|p| p.public_key == peer.public_key) {
            // add the peer to the list
            peers.push(peer);
        }
        Ok(())
    }

    /// Find new peers by queerying the existing peers
    /// and adding them to the list of peers
    async fn discover_peers(&mut self) -> Result<(), std::io::Error> {
        let mut existing_peers = self.peers.lock().await.iter()
            .map(|peer| peer.public_key)
            .collect::<HashSet<_>>();
        let mut new_peers: Vec<Peer> = Vec::new();
        // send a message to the peers
        for peer in self.peers.lock().await.iter_mut(){
            let peers = peer.communicate(
                &Message::PeerRequest, 
                Peer::new(*self.public_key, self.ip_address, *self.port)
            ).await?;
            match peers {
                Message::PeerResponse(peers) => {
                    for peer in peers{
                        // check if the peer is already in the list
                        if !existing_peers.contains(&peer.public_key){
                            // add the peer to the list
                            existing_peers.insert(peer.public_key);
                            new_peers.push(peer);
                        }
                    }
                },
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other, 
                        "Invalid message received"
                    ));
                }
            }

        }
        // extend the peers list with the new peers
        self.peers.lock().await.extend(new_peers);
        Ok(())
    }

}
