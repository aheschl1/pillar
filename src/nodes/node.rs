use super::peer::Peer;
use flume::{Receiver, Sender};
use std::{collections::{HashMap, HashSet}, net::IpAddr, ops::Deref, sync::Arc};
use tokio::sync::Mutex;

use super::messages::Message;

use crate::{
    blockchain::chain::Chain,
    crypto::signing::{DefaultSigner, SigFunction, Signable},
    persistence::database::{Datastore, EmptyDatastore},
    primitives::{block::{Block, BlockHeader, Stamp},
    pool::MinerPool,
    transaction::{FilterMatch, TransactionFilter}},
    protocol::{chain::{dicover_chain, service_sync, sync_chain},
    communication::{broadcast_knowledge, serve_peers},
    reputation::{nth_percentile_peer, N_TRANSMISSION_SIGNATURES}},
    reputation::history::NodeHistory
};
 
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum NodeState{
    ICD,
    ChainOutdated,
    ChainLoading,
    ChainSyncing,
    Serving
}

impl NodeState {
    /// If a state is_track, then the node should track incoming blocks and transactions
    /// These values should be added to chain at the soonest available moment
    /// If is_track, then the node does not add requests immediately 
    fn is_track(&self) -> bool {
        // return true;
        matches!(self, 
            NodeState::ICD | 
            NodeState::ChainSyncing | 
            NodeState::ChainLoading | 
            NodeState::ChainOutdated
        )
    }

    /// If a state is_forward, then the node should forward incoming blocks and transactions
    fn is_forward(&self) -> bool {
        // return true;
        matches!(self,
            NodeState::ICD |
            NodeState::ChainOutdated | 
            NodeState::ChainLoading | 
            NodeState::ChainSyncing | 
            NodeState::Serving
        )
    }

    /// If a state is_consume, then the node should consume incoming blocks and transactions
    /// This means that state should be updated immediately. This includes consuming a tracking queue
    fn is_consume(&self) -> bool {
        // return true;
        matches!(self, 
            NodeState::Serving
        )
    }
}

fn get_initial_state(datastore: &dyn Datastore) -> (NodeState, Option<Chain>) {
    // check if the datastore has a chain
    if datastore.latest_chain().is_some() {
        // if it does, load the chain
        match datastore.load_chain() {
            Ok(chain) => {
                // assign the chain to the node
                (NodeState::ChainOutdated, Some(chain))
            },
            Err(_) => {
                // if it fails, we are in discovery mode
                (NodeState::ICD, None)
            }
        }
    } else {
        // if it does not, we are in discovery mode
        (NodeState::ICD, None)
    }
}

pub const STANDARD_ARRAY_LENGTH: usize = 32;

pub type StdByteArray = [u8; STANDARD_ARRAY_LENGTH];

pub struct NodeInner {
    pub public_key: StdByteArray,
    /// The private key of the node
    pub private_key: StdByteArray,
    // known peers
    pub peers: Mutex<HashMap<StdByteArray, Peer>>,
    // the blockchain
    pub chain: Mutex<Option<Chain>>,
    /// transactions to be broadcasted
    pub broadcast_receiver: Receiver<Message>,
    /// transaction input
    pub broadcast_sender: Sender<Message>,
    // a collection of things already broadcasted
    pub broadcasted_already: Mutex<HashSet<StdByteArray>>,
    // transaction filter queue
    pub transaction_filters: Mutex<Vec<(TransactionFilter, Peer)>>,
    /// mapping of reputations for peers
    pub reputations: Mutex<HashMap<StdByteArray, NodeHistory>>,
    /// registered filters for the local node - producer will be this node, and consumer will be some backgroung thread that polls
    filter_callbacks: Mutex<HashMap<TransactionFilter, Sender<BlockHeader>>>,
    /// the state represents the nodes ability to communicate with other nodes
    state: Mutex<NodeState>,
    /// the datastore
    pub datastore: Option<Arc<dyn Datastore>>,
}

#[derive(Clone)]
pub struct Node {
    pub inner: Arc<NodeInner>,
    /// The IP address of the node
    pub ip_address: IpAddr,
    /// The port of the node
    pub port: u16,
    /// transactions to be serviced
    pub miner_pool: Option<MinerPool>,
}



impl Node {
    /// Create a new node
    pub fn new(
        public_key: StdByteArray,
        private_key: StdByteArray,
        ip_address: IpAddr,
        port: u16,
        peers: Vec<Peer>,
        mut database: Option<Arc<dyn Datastore>>,
        transaction_pool: Option<MinerPool>,
    ) -> Self {
        if database.is_none() {
            log::warn!("Node created without a database. This will not persist the chain or transactions.");
            database = Some(Arc::new(EmptyDatastore::new()));
        }
        let (broadcast_sender, broadcast_receiver) = flume::unbounded();
        let transaction_filters = Mutex::new(Vec::new());
        let broadcasted_already = Mutex::new(HashSet::new());
        let peer_map = peers
            .iter()
            .map(|peer| (peer.public_key, peer.clone()))
            .collect::<HashMap<_, _>>();
        let (state, maybe_chain) = get_initial_state(&**database.as_ref().unwrap());
        Node {
            inner: NodeInner {
            public_key: public_key,
            private_key: private_key,
            peers: Mutex::new(peer_map),
            chain: Mutex::new(maybe_chain),
            broadcasted_already,
            broadcast_receiver,
            broadcast_sender,
            transaction_filters,
            reputations: Mutex::new(HashMap::new()),
            filter_callbacks: Mutex::new(HashMap::new()), // initially no callbacks
            state: Mutex::new(state), // initially in discovery mode
            datastore: database,
            }.into(),
            ip_address,
            port: port,
            miner_pool: transaction_pool,
        }
    }
    
    /// when called, launches a new thread that listens for incoming connections
    pub async fn serve(&self) {
        // spawn a new thread to handle the connection
        log::info!("Node is starting up on {}:{}", self.ip_address, self.port);
        let state = self.inner.state.lock().await.clone();
        let handle = match state {
            NodeState::ICD => {
                log::info!("Starting ICD.");
                *self.inner.state.lock().await = NodeState::ChainLoading; // update state to chain loading
                let handle = tokio::spawn(dicover_chain(self.clone()));
                Some(handle)
            },
            NodeState::ChainOutdated => {
                log::info!("Node has an outdated chain. Starting sync.");
                *self.inner.state.lock().await = NodeState::ChainSyncing; // update state to chain syncing
                let handle = tokio::spawn(sync_chain(self.clone()));
                Some(handle)
            },
            _ => {
                panic!("Unexpected node state for startup: {:?}", self.inner.state);
            }
        };
        if let Some(handle) = handle {
            let self_clone = self.clone();
            tokio::spawn(async move {
                let result = handle.await;
                match result {
                    Ok(Ok(_)) => {
                        log::info!("Node started successfully.");
                        let mut state = self_clone.inner.state.lock().await;
                        // update to serving
                        *state = NodeState::Serving;
                    },
                    Ok(Err(e)) => log::error!("Node failed to start: {:?}", e),
                    Err(e) => log::error!("Failed to start node: {:?}", e),
                }

            });
        }
        tokio::spawn(serve_peers(self.clone()));
        tokio::spawn(broadcast_knowledge(self.clone()));
    }

    /// Register a transaction filter callback - adds the callback channel and adds it to the transaction filter queue
    /// Sends a broadcast to request peers to also watch for the block - if a peer catches it, it will be sent back
    pub async fn register_transaction_callback(&mut self, filter: TransactionFilter) -> Receiver<BlockHeader> {
        // create a new channel
        let (sender, receiver) = flume::bounded(1);
        // add the filter to the list
        self.inner.filter_callbacks.lock().await.insert(filter.clone(), sender);
        // add the filter to the transaction filters
        self.inner.transaction_filters.lock().await.push((filter.clone(), self.clone().into()));
        // broadcast the filter to all peers
        self.inner.broadcast_sender.send(Message::TransactionFilterRequest(filter, self.clone().into())).unwrap();
        // return the receiver
        receiver
    }

    /// Derive the response to a request from a peer
    pub async fn serve_request(&mut self, message: &Message, _declared_peer: Peer) -> Result<Message, std::io::Error> {
        let state = self.inner.state.lock().await.clone();
        match message {
            Message::PeerRequest => {
                // send all peers
                Ok(Message::PeerResponse(self.inner.peers.lock().await.values().cloned().collect()))
            }
            Message::ChainRequest => {
                if state.is_consume(){ // in consume state, chain is actively being consumed
                    Ok(Message::ChainResponse(self.inner.chain.lock().await.clone().unwrap()))
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::TransactionBroadcast(transaction) => {
                // add the transaction to the pool
                if let Some(ref pool) = self.miner_pool{
                    if state.is_consume() {
                        pool.add_transaction(transaction.clone()).await;
                    }
                }
                // to be broadcasted
                if state.is_forward(){
                    self.inner.broadcast_sender.send(Message::TransactionBroadcast(transaction.to_owned())).unwrap();
                }
                Ok(Message::TransactionAck)
            }
            Message::BlockTransmission(block) => {
                // add the block to the chain if we have downloaded it already - first it is verified TODO add to a queue to be added later
                let mut block = block.clone();
                if state.is_consume(){
                    let mut chain_lock = self.inner.chain.lock().await;
                    let chain = chain_lock.as_mut().unwrap();
                    self.settle_block(&mut block, chain).await?;
                }else{
                    // TODO handle is_track instead
                    // if we do not have the chain, just forward the block if there is room in the stamps
                    if block.header.tail.n_stamps() < N_TRANSMISSION_SIGNATURES {
                        let _ = self.stamp_block(&mut block.clone());
                        self.inner.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap();
                    }
                }
                // handle callback
                if state.is_track() {
                    self.handle_callbacks(&block).await;
                }
                Ok(Message::BlockAck)
            },
            Message::BlockRequest(hash) => {
                // send a block to the peer upon request
                if state.is_consume(){
                    let lock = self.inner.chain.lock().await;
                    let chain = lock.as_ref().unwrap();
                    let block = chain.get_block(hash).cloned();
                    Ok(Message::BlockResponse(block))
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::ChainShardRequest => {
                // send the block headers to the peer
                if state.is_consume(){
                    let lock = self.inner.chain.lock().await;
                    let chain = lock.as_ref().unwrap().clone();
                    Ok(Message::ChainShardResponse(chain.into()))
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::TransactionProofRequest(stub) => {
                if state.is_consume(){
                    let lock = self.inner.chain.lock().await;
                    let chain = lock.as_ref().unwrap().clone();

                    let block = chain.get_block(&stub.block_hash);
                    
                    if let Some(block) = block{
                        let proof = block.get_proof_for_transaction(stub.transaction_hash);
                        Ok(Message::TransactionProofResponse(proof.unwrap(), block.header))
                    }else{
                        Ok(Message::Error("Block does not exist".into()))
                    }                 
                }else{
                    Ok(Message::Error("Chain not downloaded for peer".into()))
                }
            },
            Message::TransactionFilterRequest(filter, peer) => {
                // place into the transaction filter queue - if it is not already there
                let mut transaction_filters = self.inner.transaction_filters.lock().await;
                if transaction_filters.iter().any(|(f, p)| f == filter && p == peer) {
                    self.inner.broadcast_sender.send(Message::TransactionFilterRequest(filter.to_owned(), peer.to_owned())).unwrap();
                    transaction_filters.push((filter.to_owned(), peer.to_owned()));
                }
                // to be broadcasted
                Ok(Message::TransactionFilterAck)
            },
            Message::TransactionFilterResponse(filter, header) => {
                if state.is_track(){
                    let mut callbacks = self.inner.filter_callbacks.lock().await;
                    if let Some(sender) = callbacks.get_mut(filter) {
                        // send the block header to the sender
                        sender.send(*header).unwrap();
                        // remove the callback - one time only
                        callbacks.remove(filter);
                    }
                }
                Ok(Message::TransactionFilterAck)
            },
            Message::PercentileFilteredPeerRequest(lower_n, upper_n) => {
                if state.is_consume(){
                    let reputations = self.inner.reputations.lock().await;
                    let peers = nth_percentile_peer(*lower_n, *upper_n, &reputations);
                    let peers_map = self.inner.peers.lock().await.clone();
                    // find the peer objects in the address list 
                    let filtered_peers = peers.iter().filter_map(|peer| {
                        peers_map.get(peer).cloned()
                    }).collect::<Vec<_>>();
                    Ok(Message::PercentileFilteredPeerResponse(filtered_peers))
                }else{
                    Ok(Message::PercentileFilteredPeerResponse(vec![])) // just say nothing - info not up to date
                }
            },
            Message::ChainSyncRequest(leaves) => {
                if state.is_consume() {
                    let chains = service_sync(self.clone(), leaves).await?;
                    Ok(Message::ChainSyncResponse(chains))
                }else{
                    Ok(Message::ChainSyncResponse(vec![]))
                }
            },
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Expected a request",
            )),
        }
    }

    /// After receiving a block - settle it to the chain
    /// Includes the tracking of reputation, braodcasting, and responding to callbacks
    async fn settle_block(&self, block: &mut Block, chain: &mut Chain) -> Result<(), std::io::Error> {
        if block.header.miner_address.is_some(){ // meaning it is reqrd time
            chain.add_new_block(block.clone())?; // if we pass this line, valid block
            // initialize broadcast
            self.inner.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap(); // forward
            // the stamping process is done. do, if there is a miner address, then stamping is done on this block.
            // update the reputation tracker with the block to rwared the miner
            self.maybe_create_history(block.header.miner_address.unwrap()).await;
            let mut reputations = self.inner.reputations.lock().await;
            let miner_history = reputations.get_mut(&block.header.miner_address.unwrap()).unwrap();
            miner_history.settle_head(block.header); // SETTLE TO REWARD MINING
            // REWARD STAMPERS
            // 3. reward reputation for the new stamps
            for stamp in block.header.tail.stamps.iter() {
                self.maybe_create_history(stamp.address).await;
                let history = reputations.get_mut(&stamp.address).unwrap();
                history.settle_tail(&block.header.tail, block.header); // SETTLE TO REWARD
            }
            return Ok(());
        }
        // ================================================
        // stamp and transmit
        let n_stamps = block.header.tail.n_stamps();
        let has_our_stamp = block.header.tail.stamps.iter().any(|stamp| stamp.address == self.inner.public_key);
        if n_stamps < N_TRANSMISSION_SIGNATURES && !has_our_stamp {
            let signature = self.signature_for(&block.header)?;
            let stamp = Stamp {
                address: self.inner.public_key,
                signature,
            };
            let _ = block.header.tail.stamp(stamp); // if failure, then full - doesnt matter anyways
        }
        if has_our_stamp && n_stamps == N_TRANSMISSION_SIGNATURES{
            // this means that it was already full - do NOT transmit just assign to mine
            match self.miner_pool{
                None => {},
                Some(ref pool) => {
                    // add the block to the pool
                    pool.add_ready_block(block.clone()).await;
                }
            }
        }else if n_stamps == N_TRANSMISSION_SIGNATURES - 1{
            // this means that we were the last to sign. forward and assign to mine
            self.inner.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap(); // forward
            match self.miner_pool{
                None => {},
                Some(ref pool) => {
                    // add the block to the pool
                    pool.add_ready_block(block.clone()).await;
                }
            }
        }else{
            self.inner.broadcast_sender.send(Message::BlockTransmission(block.clone())).unwrap(); // forward
        }
        Ok(())
    }

    /// spawn a new thread to match transaction callback requests against the bock
    async fn handle_callbacks(&self, block: &Block){
        let block_clone = block.clone();
        let initpeer: Peer = self.clone().into();
        let selfclone = self.clone();
        tokio::spawn(async move {
            // check filters for callback
            let mut filters = selfclone.inner.transaction_filters.lock().await;
            for ( filter, peer) in filters.iter_mut() {
                if filter.matches(&block_clone){
                    peer.communicate(&Message::TransactionFilterResponse(filter.clone(), block_clone.header), &initpeer).await.unwrap();
                    // check if there is a registered callback
                    let mut callbacks: tokio::sync::MutexGuard<'_, HashMap<TransactionFilter, Sender<BlockHeader>>> = selfclone.inner.filter_callbacks.lock().await;
                    // TODO maybe this is not nececarry - some rework?
                    if let Some(sender) = callbacks.get_mut(filter) {
                        // send the block header to the sender
                        sender.send(block_clone.header).unwrap();
                        // remove the callback - one time only
                        callbacks.remove(filter);
                    } // we will get the callback here if and only if the current active node is the one that resgistered the callback
                    // otherwise, it will come in the FilterResponse
                }
            }
        });
    }

    fn stamp_block(&self, block: &mut Block) -> Result<(), std::io::Error> {
        // sign the block with the private key
        let signature = self.signature_for(&block.header)?;
        // add the stamp to the block
        let stamp = Stamp {
            address: self.inner.public_key,
            signature: signature,
        };
        block.header.tail.stamp(stamp)
    }

    /// ed25519 signature for the node over the hash of some data
    fn signature_for<T: Signable<64>>(&self, sign: &T) -> Result<[u8; 64], std::io::Error> {
        // sign the data with the private key
        let mut signer = DefaultSigner::new(self.inner.private_key);
        let signature = signer.sign(sign);
        // return the signature
        Ok(signature)
    }

    pub async fn maybe_update_peer(&self, peer: Peer) -> Result<(), std::io::Error> {
        // check if the peer is already in the list
        let mut peers: tokio::sync::MutexGuard<'_, HashMap<StdByteArray, Peer>> = self.inner.peers.lock().await;
        if (peer.public_key != self.inner.public_key) && !peers.iter().any(|(public_key, _)| public_key == &peer.public_key) {
            // add the peer to the list
            peers.insert(peer.public_key, peer);
        }
        Ok(())
    }

    pub async fn maybe_update_peers(&self, peers: Vec<Peer>){
        for peer in peers {
            self.maybe_update_peer(peer).await.unwrap();
        }
    }

    /// create a new history for the node if it does not exist
    async fn maybe_create_history(&self, public_key: StdByteArray) {
        // get the history of the node
        let mut reputations = self.inner.reputations.lock().await;
        if reputations.get(&public_key).is_none() {
            // create a new history
            reputations.insert(public_key, NodeHistory::default()); // create new
        }
    }
}

impl From<&Node> for Peer {
    fn from(node: &Node) -> Self {
        Peer::new(node.inner.public_key, node.ip_address, node.port)
    }
}

impl From<Node> for Peer {
    fn from(node: Node) -> Self {
        Peer::new(node.inner.public_key, node.ip_address, node.port)
    }
}

pub trait Broadcaster {
    /// Broadcast a message to all peers
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error>;
}

impl Broadcaster for Node {
    async fn broadcast(&self, message: &Message) -> Result<Vec<Message>, std::io::Error> {
        // send a message to all peers
        let mut responses = Vec::new();
        let mut peers = self.inner.peers.lock().await;
        for (_, peer) in peers.iter_mut(){
            let response = peer.communicate(message, &self.into()).await;
            if let Err(e) = response {
                println!("Failed to communicate with peer {:?}: {:?}", peer.public_key, e);
                continue; // skip this peer
            }
            responses.push(response.unwrap());
        }
        Ok(responses)
    }
}

#[cfg(test)]
mod tests{

    use crate::{crypto::{hashing::{DefaultHash, HashFunction, Hashable}, signing::{DefaultSigner, SigFunction, SigVerFunction, Signable}}, nodes::{messages::Message, miner::{Miner, MAX_TRANSACTION_WAIT_TIME}, peer::Peer}, persistence::database::{Datastore, GenesisDatastore}, primitives::{block::Block, pool::MinerPool, transaction::Transaction}, protocol::{peers::discover_peers, transactions::submit_transaction}};

    use super::Node;
    use core::time;
    use std::{net::{IpAddr, Ipv4Addr}, sync::Arc};
    

    #[tokio::test]
    async fn test_create_empty_node() {
        let public_key = [0u8; 32];
        let private_key = [1u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8080;
        let peers = vec![];
        let chain = None;
        let transaction_pool = None;

        let node = Node::new(
            public_key,
            private_key,
            ip_address,
            port,
            peers,
            chain,
            transaction_pool,
        );

        assert_eq!(node.inner.public_key, public_key);
        assert_eq!(node.inner.private_key, private_key);
        assert_eq!(node.ip_address, ip_address);
        assert_eq!(node.port, port);
        assert!(node.inner.peers.lock().await.is_empty());
        assert!(node.inner.chain.lock().await.is_none());
        assert!(node.miner_pool.is_none());
        assert!(node.inner.transaction_filters.lock().await.is_empty());
        assert!(node.inner.reputations.lock().await.is_empty());
        assert!(node.inner.filter_callbacks.lock().await.is_empty());
        assert!(node.inner.state.lock().await.clone() == super::NodeState::ICD);
    }

    #[tokio::test]
    async fn test_create_empty_node_genisis() {
        let public_key = [0u8; 32];
        let private_key = [1u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8080;
        let peers = vec![];
        let datastore = Some(Arc::new(GenesisDatastore::new()));
        let transaction_pool = None;

        let node = Node::new(
            public_key,
            private_key,
            ip_address,
            port,
            peers,
            datastore.map(|ds| ds as Arc<dyn Datastore>),
            transaction_pool,
        );

        assert_eq!(node.inner.public_key, public_key);
        assert_eq!(node.inner.private_key, private_key);
        assert_eq!(node.ip_address, ip_address);
        assert_eq!(node.port, port);
        assert!(node.inner.peers.lock().await.is_empty());
        assert!(node.inner.chain.lock().await.is_some());
        assert!(node.miner_pool.is_none());
        assert!(node.inner.transaction_filters.lock().await.is_empty());
        assert!(node.inner.reputations.lock().await.is_empty());
        assert!(node.inner.filter_callbacks.lock().await.is_empty());
        assert!(node.inner.state.lock().await.clone() == super::NodeState::ChainOutdated);
    }



    #[tokio::test]
    async fn test_serve(){
        let public_key = [0u8; 32];
        let private_key = [1u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8070;
        let peers = vec![];
        let datastore = GenesisDatastore::new();
        let transaction_pool = None;

        let node = Node::new(
            public_key,
            private_key,
            ip_address,
            port,
            peers,
            Some(Arc::new(datastore)),
            transaction_pool,
        );

        node.serve().await;

        // check state update
        let state = node.inner.state.lock().await;
        assert!(*state == super::NodeState::ChainSyncing);
        drop(state);

        tokio::time::sleep(std::time::Duration::from_secs(1)).await; // wait for the node to start
        // check if the node is in serving state
        let state = node.inner.state.lock().await;
        assert!(*state == super::NodeState::Serving);

    }

    #[tokio::test]
    async fn test_two_node_chat(){
        let public_key = [0u8; 32];
        let private_key = [1u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8069;
        let peers = vec![
            Peer::new(
                [8u8; 32],
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
                8081,
            ),
        ];
        let datastore = GenesisDatastore::new();
        let transaction_pool = None;

        let node1 = Node::new(
            public_key,
            private_key,
            ip_address,
            port,
            peers,
            Some(Arc::new(datastore.clone())),
            transaction_pool,
        );

        node1.serve().await;

        // check state update
        let state = node1.inner.state.lock().await;
        assert!(*state == super::NodeState::ChainSyncing);
        drop(state);

        // create a second node
        let public_key2 = [2u8; 32];
        let private_key2 = [3u8; 32];
        let ip_address2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let port2 = 8070;
        
        let mut node2 = Node::new(
            public_key2,
            private_key2,
            ip_address2,
            port2,
            vec![node1.clone().into()],
            Some(Arc::new(datastore)),
            None,
        );

        node2.serve().await;

        // check state update
        let state = node2.inner.state.lock().await;
        assert!(*state == super::NodeState::ChainSyncing);
        drop(state);

        // attemp to make node 2 work alongside node 1
        tokio::time::sleep(std::time::Duration::from_secs(3)).await; // wait for the node to start
        // check if the node is in serving state
        let state = node1.inner.state.lock().await.clone();
        assert!(state == super::NodeState::Serving);
        let state = node2.inner.state.lock().await.clone();
        assert!(state == super::NodeState::Serving);

        // check if the nodes can communicate
        let result = discover_peers(&mut node2).await;
        result.unwrap(); // should be successful
        // make sure that node2 is in the list of peers for node 1
        let peers = node1.inner.peers.lock().await;
        assert!(peers.contains_key(&public_key2));
        drop(peers);

        // make sure the [9u8; 32] is in the list of peers for node 2
        let peers = node2.inner.peers.lock().await;
        assert!(peers.contains_key(&public_key));
        assert!(peers.contains_key(&[8u8; 32])); // the peer we added
        drop(peers);
        assert!(node2.inner.chain.lock().await.as_ref().unwrap().blocks.len() == 1);
    }

    #[tokio::test]
    async fn test_complex_peer_discovery(){
        let public_key_a = [0u8; 32];
        let private_key_a = [1u8; 32];
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port_a = 8060;

        let public_key_b = [2u8; 32];
        let private_key_b = [3u8; 32];
        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let port_b = 8061;

        let public_key_c = [4u8; 32];
        let private_key_c = [5u8; 32];
        let ip_address_c = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));
        let port_c = 8062;

        let public_key_d = [6u8; 32];
        let private_key_d = [7u8; 32];
        let ip_address_d = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 4));
        let port_d = 8063;

        let datastore = GenesisDatastore::new();

        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![Peer::new(public_key_c, ip_address_c, port_c)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let node_c = Node::new(
            public_key_c,
            private_key_c,
            ip_address_c,
            port_c,
            vec![Peer::new(public_key_d, ip_address_d, port_d)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let mut node_d = Node::new(
            public_key_d,
            private_key_d,
            ip_address_d,
            port_d,
            vec![Peer::new(public_key_a, ip_address_a, port_a)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        node_d.serve().await; // no discovery
        node_c.serve().await; // D learns of C
        node_b.serve().await; // C learns of B
        node_a.serve().await; // B learns of A

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;        
        discover_peers(&mut node_a).await.unwrap(); // 
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(peers_a.contains_key(&public_key_c));
        assert!(!peers_a.contains_key(&public_key_d)); // node A does not know about D yet
        drop(peers_a);

        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&public_key_a));
        assert!(peers_b.contains_key(&public_key_c));
        assert!(!peers_b.contains_key(&public_key_d));
        drop(peers_b);

        let peers_c = node_c.inner.peers.lock().await;
        assert!(!peers_c.contains_key(&public_key_a));
        assert!(peers_c.contains_key(&public_key_b));
        assert!(peers_c.contains_key(&public_key_d));
        drop(peers_c);

        let peers_d = node_d.inner.peers.lock().await;
        assert!(peers_d.contains_key(&public_key_a));
        assert!(!peers_d.contains_key(&public_key_b));
        assert!(peers_d.contains_key(&public_key_c));
        drop(peers_d);

        // D does not know of B.
        // A does not know of D.
        // C does not know of A.
        // B does not know of D.

        discover_peers(&mut node_d).await.unwrap(); // 
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(peers_a.contains_key(&public_key_c));
        assert!(!peers_a.contains_key(&public_key_a));
        assert!(peers_a.contains_key(&public_key_d)); // node A does not know about D yet
        drop(peers_a);

        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&public_key_a));
        assert!(peers_b.contains_key(&public_key_c));
        assert!(!peers_b.contains_key(&public_key_d));
        drop(peers_b);

        let peers_c = node_c.inner.peers.lock().await;
        assert!(!peers_c.contains_key(&public_key_a));
        assert!(peers_c.contains_key(&public_key_b));
        assert!(peers_c.contains_key(&public_key_d));
        drop(peers_c);

        let peers_d = node_d.inner.peers.lock().await;
        assert!(peers_d.contains_key(&public_key_a));
        assert!(peers_d.contains_key(&public_key_b));
        assert!(peers_d.contains_key(&public_key_c));
        drop(peers_d);
        
    }

    #[tokio::test]
    async fn test_start_account(){
        let mut signing_a = DefaultSigner::generate_random();
        let public_key_a = signing_a.get_verifying_function().to_bytes();
        let private_key_a = signing_a.to_bytes();

        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port_a = 8020;

        let signing_b = DefaultSigner::generate_random();
        let public_key_b = signing_b.get_verifying_function().to_bytes();
        let private_key_b = signing_b.to_bytes();

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let port_b = 8021;

        let datastore = GenesisDatastore::new();

        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![],
            Some(Arc::new(datastore.clone())),
            None,
        );

        node_b.serve().await;
        node_a.serve().await;

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        
        // Node A sends a peer request to Node B
        discover_peers(&mut node_a).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Verify that Node B knows Node A
        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&public_key_a));

        drop(peers_b);

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(!peers_a.contains_key(&public_key_a)); // Node A does not know itself
        drop(peers_a);
        // now, A makes a transaction of 0 dollars to B

        let mut chain = node_a.inner.chain.lock().await;
        let sender_account = chain.as_mut().unwrap().account_manager.get_or_create_account(&public_key_a);
        drop(chain);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let result = submit_transaction(
            &mut node_a, 
            &mut sender_account.lock().unwrap(), 
            &mut signing_a, 
            public_key_b, 
            0, 
            false,
            Some(timestamp)
        ).await.unwrap();

        assert!(result.is_none());

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        
        // node a already broadcasted
        
        let mut transaction = Transaction::new(
            sender_account.lock().unwrap().address, 
            public_key_b, 
            0, 
            timestamp,
            0, 
            &mut DefaultHash::new()
        );
        transaction.sign(&mut signing_a);

        let message_to_hash = Message::TransactionBroadcast(transaction);

        // also, node b should be forward by now
        assert!(node_b.inner.state.lock().await.is_forward());

        let already_b = node_a.inner.broadcasted_already.lock().await;
        assert!(already_b.contains(&message_to_hash.hash(&mut DefaultHash::new()).unwrap()));
        drop(already_b);
        let already_b = node_b.inner.broadcasted_already.lock().await;
        assert!(already_b.contains(&message_to_hash.hash(&mut DefaultHash::new()).unwrap()));

        // at this point, everything is broadcasted - and recorded. this means we had no loop, we are happy.

        // make sure the accounts do NOT exist for the receiver - because it has never been settled.
        let chain_a = node_a.inner.chain.lock().await;
        let account_a = chain_a.as_ref().unwrap().account_manager.get_account(&public_key_a).unwrap();
        let account_b = chain_a.as_ref().unwrap().account_manager.get_account(&public_key_b);
        assert!(account_b.is_none());
        assert_eq!(account_a.lock().unwrap().balance, 0); // because we made it
        drop(chain_a);
        let chain_b = node_b.inner.chain.lock().await;
        let account_b = chain_b.as_ref().unwrap().account_manager.get_account(&public_key_b);
        let account_a = chain_b.as_ref().unwrap().account_manager.get_account(&public_key_a);
        assert!(account_b.is_none());
        assert!(account_a.is_none());  // on node b, it has no record of either yet as the transactions are not in blocks
        drop(chain_b);

    }

    #[tokio::test]
    async fn test_transaction_and_block_proposal(){

        let mut signing_a = DefaultSigner::generate_random();
        let public_key_a = signing_a.get_verifying_function().to_bytes();
        let private_key_a = signing_a.to_bytes();

        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 9));
        let port_a = 8020;

        let signing_b = DefaultSigner::generate_random();
        let public_key_b = signing_b.get_verifying_function().to_bytes();
        let private_key_b = signing_b.to_bytes();

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 8));
        let port_b = 8021;

        let datastore = GenesisDatastore::new();

        let mut node_a = Node::new(
            public_key_a,
            private_key_a,
            ip_address_a,
            port_a,
            vec![Peer::new(public_key_b, ip_address_b, port_b)],
            Some(Arc::new(datastore.clone())),
            None,
        );

        let node_b = Node::new(
            public_key_b,
            private_key_b,
            ip_address_b,
            port_b,
            vec![],
            Some(Arc::new(datastore.clone())),
            Some(MinerPool::new()), // we will mine from this node
        );

        let miner_b = Miner::new(node_b.clone()).unwrap();

        miner_b.serve().await;
        node_a.serve().await;

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Node A sends a peer request to Node B
        println!("Node A starting discovery");

        discover_peers(&mut node_a).await.unwrap();
        println!("Node A discovered Node B");

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Verify that Node B knows Node A
        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&public_key_a));

        drop(peers_b);

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(!peers_a.contains_key(&public_key_a)); // Node A does not know itself
        drop(peers_a);
        // now, A makes a transaction of 0 dollars to B


        let mut chain = node_a.inner.chain.lock().await;
        let sender_account = chain.as_mut().unwrap().account_manager.get_or_create_account(&public_key_a);
        drop(chain);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        let result = submit_transaction(
            &mut node_a, 
            &mut sender_account.lock().unwrap(), 
            &mut signing_a, 
            public_key_b, 
            0, 
            false,
            Some(timestamp)
        ).await.unwrap();

        assert!(result.is_none());

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        
        // node a already broadcasted

        let mut transaction = Transaction::new(
            sender_account.lock().unwrap().address, 
            public_key_b, 
            0, 
            timestamp,
            0, 
            &mut DefaultHash::new()
        );
        transaction.sign(&mut signing_a);

        let message_to_hash = Message::TransactionBroadcast(transaction);

        // also, node b should be forward by now

        assert!(node_b.inner.state.lock().await.is_forward());

        let already_b = node_a.inner.broadcasted_already.lock().await;
        assert!(already_b.contains(&message_to_hash.hash(&mut DefaultHash::new()).unwrap()));
        assert_eq!(already_b.len(), 1);
        drop(already_b);
        let already_b = node_b.inner.broadcasted_already.lock().await;
        assert!(already_b.contains(&message_to_hash.hash(&mut DefaultHash::new()).unwrap()));
        assert_eq!(already_b.len(), 1);
        drop(already_b);

        // at this point, everything is broadcasted - and recorded. this means we had no loop, we are happy.

        // make sure the accounts do NOT exist for the receiver - because it has never been settled.

        let chain_a = node_a.inner.chain.lock().await;
        let account_a = chain_a.as_ref().unwrap().account_manager.get_account(&public_key_a).unwrap();
        let account_b = chain_a.as_ref().unwrap().account_manager.get_account(&public_key_b);
        assert!(account_b.is_none());
        assert_eq!(account_a.lock().unwrap().balance, 0); // because we made it
        drop(chain_a);
        let chain_b = node_b.inner.chain.lock().await;
        let account_b = chain_b.as_ref().unwrap().account_manager.get_account(&public_key_b);
        let account_a = chain_b.as_ref().unwrap().account_manager.get_account(&public_key_a);
        assert!(account_b.is_none());
        assert!(account_a.is_none());  // on node b, it has no record of either yet as the transactions are not in blocks
        drop(chain_b);

        // miners wait 10 seconds for more transactions to come in
        println!("waiting for miner to process transactions");
        tokio::time::sleep(std::time::Duration::from_secs(MAX_TRANSACTION_WAIT_TIME)).await;
        // now, by this time it should have been consumed into a block proposition.
        // this block proposition will be sent out from node_b to node_a.
        // we will check the already broadcasted messages first.
        // we cannot know the exact hash, but check that 2 are in there
        let already_b = node_b.inner.broadcasted_already.lock().await;
        assert_eq!(already_b.len(), 3); // at least the transaction and the block
        drop(already_b);
        let already_b = node_a.inner.broadcasted_already.lock().await;
        assert_eq!(already_b.len(), 3); // at least the transaction and the block
        drop(already_b);

    }
        
}