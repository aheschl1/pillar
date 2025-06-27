pub mod messages;
pub mod miner;
pub mod node;
pub mod peer;

#[cfg(test)]
mod tests {

    use chrono::Local;
    use pillar_crypto::{hashing::{DefaultHash, Hashable}, signing::{SigFunction, Signable}, types::StdByteArray};
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::{
        Layer, Registry,
        fmt::{self, writer::BoxMakeWriter},
        layer::SubscriberExt,
        util::SubscriberInitExt,
    };

    use crate::{
        accounting::wallet::Wallet, nodes::{
            messages::Message, miner::{Miner, MAX_TRANSACTION_WAIT_TIME}, node::NodeState, peer::Peer
        }, persistence::database::{Datastore, EmptyDatastore, GenesisDatastore}, primitives::{pool::MinerPool, transaction::Transaction}, protocol::{difficulty::get_reward_from_depth_and_stampers, peers::discover_peers, transactions::submit_transaction}
    };

    use super::node::Node;

    use std::{
        fs::File,
        net::{IpAddr, Ipv4Addr},
        sync::Arc,
    };

    // always setup tracing first
    #[ctor::ctor]
    fn setup() {
        // === Setup folder structure under ./test_output/{timestamp} ===
        let timestamp = Local::now().format("%d_%H-%M-%S").to_string();
        let log_dir = format!("./test_output/{}", timestamp);
        std::fs::create_dir_all(&log_dir).expect("failed to create log directory");

        let filename = format!("{log_dir}/output.log");
        let file = File::create(filename).expect("failed to create log file");

        // === Console output for WARN and above ===
        let console_layer = fmt::layer()
            .with_ansi(true)
            .with_level(true)
            .with_filter(LevelFilter::ERROR);

        let file_layer = fmt::layer()
            .with_writer(BoxMakeWriter::new(file))
            .with_ansi(false)
            .with_level(true)
            .with_filter(LevelFilter::DEBUG);

        // === Combined subscriber ===
        Registry::default()
            .with(file_layer)
            .with(console_layer)
            .try_init();
    }

    async fn create_empty_node_genisis(
        ip_address: IpAddr, 
        port: u16, 
        peers: Vec<Peer>,
        genesis_store: bool,
        miner_pool: Option<MinerPool>,
    ) -> (Node, Wallet){
        let wallet = Wallet::generate_random();


        let node = Node::new(
            wallet.address,
            wallet.get_private_key(),
            ip_address,
            port,
            peers.clone(),
            if genesis_store {Some(Arc::new(GenesisDatastore::new()))} else {None},
            miner_pool.clone(),
        );

        assert_eq!(node.inner.public_key, wallet.address);
        assert_eq!(node.inner.private_key, wallet.get_private_key());
        assert_eq!(node.ip_address, ip_address);
        assert_eq!(node.port, port);
        assert_eq!(node.inner.peers.lock().await.len(), peers.len());
        assert_eq!(node.inner.chain.lock().await.is_some(), genesis_store);
        assert_eq!(node.miner_pool.is_some(), miner_pool.is_some());
        assert!(node.inner.transaction_filters.lock().await.is_empty());
        assert!(node.inner.filter_callbacks.lock().await.is_empty());
        if genesis_store{
            assert_eq!(node.inner.state.lock().await.clone(), NodeState::ChainOutdated);
        }else{
            assert_eq!(node.inner.state.lock().await.clone(), NodeState::ICD);
        }

        (node, wallet)
    }

    #[tokio::test]
    async fn test_serve() {
        let public_key = [0u8; 32];
        let private_key = [1u8; 32];
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8070;
        let peers = vec![];
        let datastore = GenesisDatastore::new();
        let transaction_pool = None;

        let mut node = Node::new(
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
        assert!(*state == NodeState::ChainSyncing);
        drop(state);

        tokio::time::sleep(std::time::Duration::from_secs(1)).await; // wait for the node to start
        // check if the node is in serving state
        let state = node.inner.state.lock().await;
        assert!(*state == NodeState::Serving);
    }

    #[tokio::test]
    async fn test_two_node_chat() {
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 8069;
        let peers = vec![Peer::new(
            [8u8; 32],
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
            8081,
        )];

        let (mut node1, wallet1) = create_empty_node_genisis(
            ip_address,
            port,
            peers,
            true,
            None,
        )
        .await;

        node1.serve().await;

        // check state update
        let state = node1.inner.state.lock().await;
        assert!(*state == NodeState::ChainSyncing);
        drop(state);

        // create a second node

        let (mut node2, wallet2) = create_empty_node_genisis(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
            8070,
            vec![node1.clone().into()],
            true,
            None,
        )
        .await;

        node2.serve().await;

        // check state update
        let state = node2.inner.state.lock().await;
        assert!(*state == NodeState::ChainSyncing);
        drop(state);

        // attempt to make node 2 work alongside node 1
        tokio::time::sleep(std::time::Duration::from_secs(3)).await; // wait for the node to start
        // check if the node is in serving state
        let state = node1.inner.state.lock().await.clone();
        assert!(state == NodeState::Serving);
        let state = node2.inner.state.lock().await.clone();
        assert!(state == NodeState::Serving);

        // check if the nodes can communicate
        let result = discover_peers(&mut node2).await;
        result.unwrap(); // should be successful
        // make sure that node2 is in the list of peers for node 1
        let peers = node1.inner.peers.lock().await;
        assert!(peers.contains_key(&wallet2.address));
        drop(peers);

        // make sure the [9u8; 32] is in the list of peers for node 2
        let peers = node2.inner.peers.lock().await;
        assert!(peers.contains_key(&wallet1.address));
        assert!(peers.contains_key(&[8u8; 32])); // the peer we added
        drop(peers);
        assert_eq!(node2.inner.chain.lock().await.as_ref().unwrap().blocks.len(), 1);
    }

    #[tokio::test]
    async fn test_complex_peer_discovery() {
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port_a = 8060;

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let port_b = 8061;

        let ip_address_c = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));
        let port_c = 8062;

        let ip_address_d = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 4));
        let port_d = 8063;


        let (mut node_a, wallet_a) = create_empty_node_genisis(
            ip_address_a,
            port_a,
            vec![],
            true,
            None,
        )
        .await;

        let (mut node_d, wallet_d) = create_empty_node_genisis(
            ip_address_d,
            port_d,
            vec![Peer::new(wallet_a.address, ip_address_a, port_a)],
            true,
            None,
        )
        .await;

        let (mut node_c, wallet_c) = create_empty_node_genisis(
            ip_address_c,
            port_c,
            vec![Peer::new(wallet_d.address, ip_address_d, port_d)],
            true,
            None,
        )
        .await;


        let (mut node_b, wallet_b) = create_empty_node_genisis(
            ip_address_b,
            port_b,
            vec![Peer::new(wallet_c.address, ip_address_c, port_c)],
            true,
            None,
        )
        .await;

        // nned to ammend b into a
        node_a.inner.peers.lock().await.insert(
            wallet_b.address,
            Peer::new(wallet_b.address, ip_address_b, port_b),
        );

        node_d.serve().await; // no discovery
        node_c.serve().await; // D learns of C
        node_b.serve().await; // C learns of B
        node_a.serve().await; // B learns of A

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        discover_peers(&mut node_a).await.unwrap(); // 
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&wallet_b.address));
        assert!(peers_a.contains_key(&wallet_c.address));
        assert!(!peers_a.contains_key(&wallet_d.address)); // node A does not know about D yet
        drop(peers_a);

        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&wallet_a.address));
        assert!(peers_b.contains_key(&wallet_c.address));
        assert!(!peers_b.contains_key(&wallet_d.address));
        drop(peers_b);

        let peers_c = node_c.inner.peers.lock().await;
        assert!(!peers_c.contains_key(&wallet_a.address));
        assert!(peers_c.contains_key(&wallet_b.address));
        assert!(peers_c.contains_key(&wallet_d.address));
        drop(peers_c);

        let peers_d = node_d.inner.peers.lock().await;
        assert!(peers_d.contains_key(&wallet_a.address));
        assert!(!peers_d.contains_key(&wallet_b.address));
        assert!(peers_d.contains_key(&wallet_c.address));
        drop(peers_d);

        // D does not know of B.
        // A does not know of D.
        // C does not know of A.
        // B does not know of D.

        discover_peers(&mut node_d).await.unwrap(); // 
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&wallet_b.address));
        assert!(peers_a.contains_key(&wallet_c.address));
        assert!(!peers_a.contains_key(&wallet_a.address));
        assert!(peers_a.contains_key(&wallet_d.address)); // node A does not know about D yet
        drop(peers_a);

        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&wallet_a.address));
        assert!(peers_b.contains_key(&wallet_c.address));
        assert!(!peers_b.contains_key(&wallet_d.address));
        drop(peers_b);

        let peers_c = node_c.inner.peers.lock().await;
        assert!(!peers_c.contains_key(&wallet_a.address));
        assert!(peers_c.contains_key(&wallet_b.address));
        assert!(peers_c.contains_key(&wallet_d.address));
        drop(peers_c);

        let peers_d = node_d.inner.peers.lock().await;
        assert!(peers_d.contains_key(&wallet_a.address));
        assert!(peers_d.contains_key(&wallet_b.address));
        assert!(peers_d.contains_key(&wallet_c.address));
        drop(peers_d);
    }

    #[tokio::test]
    async fn test_start_account() {

        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port_a = 8020;

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let port_b = 8021;



        let (mut node_b, wallet_b) = create_empty_node_genisis(
            ip_address_b,
            port_b,
            vec![],
            true,
            None,
        )
        .await;

        let (mut node_a, mut wallet_a) = create_empty_node_genisis(
            ip_address_a,
            port_a,
            vec![Peer::new(wallet_b.address, ip_address_b, port_b)],
            true,
            None,
        )
        .await;

        node_b.serve().await;
        node_a.serve().await;

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Node A sends a peer request to Node B
        discover_peers(&mut node_a).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Verify that Node B knows Node A
        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&wallet_a.address));

        drop(peers_b);

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&wallet_b.address));
        assert!(!peers_a.contains_key(&wallet_a.address)); // Node A does not know itself
        drop(peers_a);
        // now, A makes a transaction of 0 dollars to B

        let chain = node_a.inner.chain.lock().await;
        drop(chain);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let result = submit_transaction(
            &mut node_a,
            &mut wallet_a,
            wallet_b.address,
            0,
            false,
            Some(timestamp),
        )
        .await
        .unwrap();

        assert!(result.is_none());

        tokio::time::sleep(std::time::Duration::from_secs(MAX_TRANSACTION_WAIT_TIME)).await;

        // node a already broadcasted

        let mut transaction = Transaction::new(
            wallet_a.address,
            wallet_b.address,
            0,
            timestamp,
            0,
            &mut DefaultHash::new(),
        );
        transaction.sign(&mut wallet_a);

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
        assert_eq!(chain_a.as_ref().unwrap().depth, 0); // NO MINER 
        let state_root = chain_a.as_ref().unwrap().get_state_root().unwrap();
        let account_a = chain_a
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&wallet_a.address, state_root);

        let account_b = chain_a
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&wallet_b.address, state_root);
        assert!(account_b.is_none());
        assert!(account_a.is_none());
        drop(chain_a);
        let chain_b = node_b.inner.chain.lock().await;
        let state_root = chain_b.as_ref().unwrap().get_state_root().unwrap();
        let account_b = chain_b
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&wallet_b.address, state_root);
        let account_a = chain_b
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&wallet_a.address, state_root);
        assert!(account_b.is_none());
        assert!(account_a.is_none()); // on node b, it has no record of either yet as the transactions are not in blocks
        drop(chain_b);
    }

    async fn inner_test_transaction_and_block_proposal(
        node_a: &mut Node,
        node_b: &Node,
        wallet_a: &mut Wallet,
        public_key_b: StdByteArray,
    ) {
        let mut miner_b = Miner::new(node_b.clone()).unwrap();

        miner_b.serve().await;
        node_a.serve().await;

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Node A sends a peer request to Node B
        println!("Node A starting discovery");

        discover_peers(node_a).await.unwrap();
        println!("Node A discovered Node B");

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Verify that Node B knows Node A
        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&wallet_a.address));

        drop(peers_b);

        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&public_key_b));
        assert!(!peers_a.contains_key(&wallet_a.address)); // Node A does not know itself
        drop(peers_a);
        // now, A makes a transaction of 0 dollars to B

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let result = submit_transaction(
            node_a,
            wallet_a,
            public_key_b,
            0,
            false,
            Some(timestamp),
        )
        .await
        .unwrap();

        assert!(result.is_none());

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // node a already broadcasted

        let mut transaction = Transaction::new(
            wallet_a.address,
            public_key_b,
            0,
            timestamp,
            0,
            &mut DefaultHash::new(),
        );
        transaction.sign(wallet_a);

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
        let state_root = chain_a.as_ref().unwrap().get_state_root().unwrap();
        let account_a = chain_a
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&wallet_a.address, state_root);
        let account_b = chain_a
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&public_key_b, state_root);
        assert!(account_b.is_none());
        assert!(account_a.is_none()); // because we made it
        drop(chain_a);
        let chain_b = node_b.inner.chain.lock().await;
        let state_root = chain_b.as_ref().unwrap().get_state_root().unwrap();
        let account_b = chain_b
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&public_key_b,state_root);
        let account_a = chain_b
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&wallet_a.address,state_root);
        assert!(account_b.is_none());
        assert!(account_a.is_none()); // on node b, it has no record of either yet as the transactions are not in blocks
        drop(chain_b);

        // miners wait 10 seconds for more transactions to come in
        println!("waiting for miner to process transactions");
        tokio::time::sleep(std::time::Duration::from_secs(MAX_TRANSACTION_WAIT_TIME)).await;
        // now, by this time it should have been consumed into a block proposition.
        // this block proposition will be sent out from node_b to node_a.
        // we will check the already broadcasted messages first.
        // we cannot know the exact hash, but check that 2 are in there
        let already_b = node_b.inner.broadcasted_already.lock().await;
        assert_eq!(already_b.len(), 4); // at least the transaction and the block. also the mined block
        drop(already_b);
        let already_b = node_a.inner.broadcasted_already.lock().await;
        assert_eq!(already_b.len(), 4); // at least the transaction and the block
        drop(already_b);

        // assert the block is in the chain of node_b
        let chain_b = node_b.inner.chain.lock().await;
        let state_root = chain_b.as_ref().unwrap().get_state_root().unwrap();
        assert_eq!(chain_b.as_ref().unwrap().depth, 1); // 2 blocks
        assert_eq!(chain_b.as_ref().unwrap().blocks.len(), 2); // 2 blocks
        let history_ba = chain_b.as_ref().unwrap().state_manager.get_account_or_default(&wallet_a.address, state_root).history;
        let history_bb = chain_b.as_ref().unwrap().state_manager.get_account_or_default(&public_key_b, state_root).history;

        assert!(history_ba.is_some());
        assert!(history_bb.is_some());

        let history_ba = history_ba.unwrap();
        let history_bb = history_bb.unwrap();

        drop(chain_b);
        // assert the block is in the chain of node_a
        let mut chain_a = node_a.inner.chain.lock().await;
        assert_eq!(chain_a.as_ref().unwrap().depth, 1); // 2 blocks
        assert_eq!(chain_a.as_ref().unwrap().blocks.len(), 2); // 2 blocks
        let history_aa = chain_a.as_mut().unwrap().state_manager.get_account_or_default(&wallet_a.address, state_root).history;
        let history_ab = chain_a.as_mut().unwrap().state_manager.get_account_or_default(&public_key_b, state_root).history;

        let history_aa = history_aa.unwrap();
        let history_ab = history_ab.unwrap();

        drop(chain_a);
        // check reputations.

        assert_eq!(history_ba.n_blocks_mined(), 0); // 0 blocks mined
        assert_eq!(history_ba.n_blocks_stamped(), 1); // 0 blocks stamped
        let b_a = history_ba.compute_reputation();

        assert_eq!(history_bb.n_blocks_mined(), 1); // 1 block mined
        assert_eq!(history_bb.n_blocks_stamped(), 1); // 0 blocks stamped
        let b_b = history_bb.compute_reputation();

        assert_eq!(history_aa.n_blocks_mined(), 0); // 0 blocks mined
        assert_eq!(history_aa.n_blocks_stamped(), 1); // 0 blocks stamped
        let a_a = history_aa.compute_reputation();
        assert_eq!(history_ab.n_blocks_mined(), 1); // 1 block mined
        assert_eq!(history_ab.n_blocks_stamped(), 1); // 0 blocks stamped
        let a_b = history_ab.compute_reputation();

        assert!(a_a == b_a);
        assert!(a_b == b_b);
        assert!(a_b > a_a);

        // check miner got paid
        let chain_b = node_b.inner.chain.lock().await;
        let state_root = chain_b.as_ref().unwrap().get_state_root().unwrap();
        let account_b = chain_b
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&public_key_b, state_root)
            .unwrap();
        assert!(account_b.balance > 0); // miner got paid
        let balance_b = account_b.balance;
        drop(chain_b);
        let chain_a = node_a.inner.chain.lock().await;
        let state_root = chain_a.as_ref().unwrap().get_state_root().unwrap();
        let account_b = chain_a
            .as_ref()
            .unwrap()
            .state_manager
            .get_account(&public_key_b, state_root)
            .unwrap();
        assert!(account_b.balance > 0); // miner got paid
        assert_eq!(account_b.balance, balance_b); // balance is the same on both nodes
        drop(chain_a);
    }

    #[tokio::test]
    async fn test_transaction_and_block_proposal() {
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 9));
        let port_a = 8020;

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 8));
        let port_b = 8021;


        let (node_b, wallet_b) = create_empty_node_genisis(
            ip_address_b,
            port_b,
            vec![],
            true,
            Some(MinerPool::new()),
        )
        .await;

        let (mut node_a, mut wallet_a) = create_empty_node_genisis(
            ip_address_a,
            port_a,
            vec![Peer::new(wallet_b.address, ip_address_b, port_b)],
            true,
            None,
        )
        .await;

        inner_test_transaction_and_block_proposal(
            &mut node_a,
            &node_b,
            &mut wallet_a,
            wallet_b.address,
        )
        .await;
    }

    #[tokio::test]
    async fn test_double_transaction() {
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));
        let port_a = 8020;

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 4));
        let port_b = 8021;

        let datastore = GenesisDatastore::new();
        
        let (mut node_b, mut wallet_b) = create_empty_node_genisis(
            ip_address_b,
            port_b,
            vec![],
            true,
            Some(MinerPool::new()), // we will mine from this node
        ).await;

        let (mut node_a, mut wallet_a) = create_empty_node_genisis(
           ip_address_a,
            port_a,
            vec![Peer::new(wallet_b.address, ip_address_b, port_b)],
            true,
            None,
        ).await;

        inner_test_transaction_and_block_proposal(
            &mut node_a,
            &node_b,
            &mut wallet_a,
            wallet_b.address,
        )
        .await;

        // one transaction has passed, now we will try to make another one
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
            
        let result = submit_transaction(
            &mut node_b,
            &mut wallet_b,
            wallet_a.address,
            0,
            false,
            Some(timestamp),
        )
        .await
        .unwrap();
        assert!(result.is_none());
        // at this point, there should be 8 total broadcasts each - after a pause
        tokio::time::sleep(std::time::Duration::from_secs(
            MAX_TRANSACTION_WAIT_TIME + 5,
        ))
        .await;
        let already_b = node_b.inner.broadcasted_already.lock().await;
        assert!(already_b.len() == 8);
        drop(already_b);
        let already_b = node_a.inner.broadcasted_already.lock().await;
        assert!(already_b.len() == 8);
        drop(already_b);

        // now, make sure the chains are length 2
        let chain_b = node_b.inner.chain.lock().await;
        let state_root = chain_b.as_ref().unwrap().get_state_root().unwrap();

        assert!(chain_b.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_b.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        let history_ba = chain_b.as_ref().unwrap().state_manager.get_account_or_default(&wallet_a.address, state_root).history.unwrap();
        let history_bb = chain_b.as_ref().unwrap().state_manager.get_account_or_default(&wallet_b.address, state_root).history.unwrap();
        drop(chain_b);
        // assert the block is in the chain of node_a
        let chain_a = node_a.inner.chain.lock().await;
        assert!(chain_a.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_a.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        let history_aa = chain_a.as_ref().unwrap().state_manager.get_account_or_default(&wallet_a.address, state_root).history;
        let history_ab = chain_a.as_ref().unwrap().state_manager.get_account_or_default(&wallet_b.address, state_root).history;

        let history_aa = history_aa.unwrap();
        let history_ab = history_ab.unwrap();
        drop(chain_a);
        // check reputations.
        assert_eq!(history_ba.n_blocks_mined(), 0); // 0 blocks mined
        assert_eq!(history_ba.n_blocks_stamped(), 2); // 2 blocks stamped
        let b_a = history_ba.compute_reputation();
        assert_eq!(history_bb.n_blocks_mined(), 2); // 2 blocks mined
        assert_eq!(history_bb.n_blocks_stamped(), 2); // 2 blocks stamped
        let b_b = history_bb.compute_reputation();
        assert_eq!(history_aa.n_blocks_mined(), 0); // 0 blocks mined
        assert_eq!(history_aa.n_blocks_stamped(), 2); // 2 blocks stamped
        let a_a = history_aa.compute_reputation();
        assert_eq!(history_ab.n_blocks_mined(), 2); // 2 blocks mined
        assert_eq!(history_ab.n_blocks_stamped(), 2); // 2 blocks stamped
        let a_b = history_ab.compute_reputation();
        assert!(a_a == b_a);
        assert!(a_b == b_b);
        assert!(a_b > a_a); // B has more reputation than A
    }

    /// Launches three nodes - matching the third one in delay
    /// There are two blocks - genesis + 1.
    /// They are synced.
    async fn inner_test_new_node_online(
        node_a: &mut Node,
        node_b: &mut Node,
        node_c: &mut Node,
        wallet_a: &mut Wallet,
        wallet_b: &Wallet,
        wallet_c: &Wallet,

    ) {
        inner_test_transaction_and_block_proposal(
            node_a,
            node_b,
            wallet_a,
            wallet_b.address,
        )
        .await;

        // node c serves
        assert!(node_c.inner.state.lock().await.clone() == NodeState::ICD);
        node_c.serve().await;
        // now, the node should launch chain discovery
        assert!(node_c.inner.state.lock().await.clone() == NodeState::ChainLoading);
        // wait for a bit
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // check if node c knows about node a and b
        let peers_c = node_c.inner.peers.lock().await;
        assert!(peers_c.contains_key(&wallet_a.address));
        assert!(peers_c.contains_key(&wallet_b.address));
        assert!(!peers_c.contains_key(&wallet_c.address)); // node c does not know about itself
        drop(peers_c);
        // make sure chain is length 3
        let chain_c = node_c.inner.chain.lock().await;
        let state_root = chain_c.as_ref().unwrap().get_state_root().unwrap();
        assert!(chain_c.as_ref().unwrap().depth == 1); // 2 blocks
        assert!(chain_c.as_ref().unwrap().blocks.len() == 2); // 3 blocks
        let history_a = chain_c.as_ref().unwrap().state_manager.get_account_or_default(&wallet_a.address, state_root).history;
        let history_b = chain_c.as_ref().unwrap().state_manager.get_account_or_default(&wallet_b.address, state_root).history;
        let history_c = chain_c.as_ref().unwrap().state_manager.get_account_or_default(&wallet_c.address, state_root).history;
        let history_a = history_a.unwrap();
        let history_b = history_b.unwrap();
        assert!(history_c.is_none()); // node c has no history yet, as it is not stamped
        let chain_a = node_a.inner.chain.lock().await;
        // check hash equalities
        let mut ablocks = chain_a
            .as_ref()
            .unwrap()
            .blocks
            .values()
            .collect::<Vec<_>>();
        ablocks.sort_by_key(|b| b.header.depth);
        let mut cblocks = chain_c
            .as_ref()
            .unwrap()
            .blocks
            .values()
            .collect::<Vec<_>>();
        cblocks.sort_by_key(|b| b.header.depth);

        assert_eq!(ablocks[0].hash.unwrap(), cblocks[0].hash.unwrap(),);
        assert_eq!(ablocks[1].hash.unwrap(), cblocks[1].hash.unwrap(),);
        drop(chain_c);
        drop(chain_a);
        assert!(node_c.inner.state.lock().await.clone() == NodeState::Serving);
        // make sure reputations are correct
        assert_eq!(history_a.n_blocks_mined(), 0); // 0 blocks mined
        assert_eq!(history_a.n_blocks_stamped(), 1); // 2 blocks stamped
        let c_a = history_a.compute_reputation();
        assert_eq!(history_b.n_blocks_mined(), 1); // 2 blocks mined
        assert_eq!(history_b.n_blocks_stamped(), 1); // 2 blocks stamped
        let c_b = history_b.compute_reputation();
        assert!(c_a < c_b);
    }

    #[tokio::test]
    async fn test_new_node_online() {
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));
        let port_a = 8015;

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 4));
        let port_b = 8016;

        let ip_address_c = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 5));
        let port_c = 8022;

        let (mut node_b, wallet_b) = create_empty_node_genisis(
            ip_address_b,
            port_b,
            vec![],
            true,
            Some(MinerPool::new()),
        )
        .await;

        let (mut node_a, mut wallet_a) = create_empty_node_genisis(
            ip_address_a,
            port_a,
            vec![Peer::new(wallet_b.address, ip_address_b, port_b)],
            true,
            None,
        )
        .await;

        let (mut node_c, wallet_c) = create_empty_node_genisis(
            ip_address_c,
            port_c,
            vec![Peer::new(wallet_a.address, ip_address_a, port_a)],
            false,
            None,
        )
        .await;

        inner_test_new_node_online(
            &mut node_a,
            &mut node_b,
            &mut node_c,
            &mut wallet_a,
            &wallet_b,
            &wallet_c,
        )
        .await;
    }

    #[tokio::test]
    async fn test_sync_later() {
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));
        let port_a = 8008;

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 4));
        let port_b = 8009;

        let ip_address_c = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 5));
        let port_c = 8010;


        let (mut node_b, wallet_b) = create_empty_node_genisis(
            ip_address_b,
            port_b,
            vec![],
            true,
            Some(MinerPool::new()),
        )
        .await;

        let (mut node_a, mut wallet_a) = create_empty_node_genisis(
            ip_address_a,
            port_a,
            vec![Peer::new(wallet_b.address, ip_address_b, port_b)],
            true,
            None,
        )
        .await;

        let (mut node_c, mut wallet_c) = create_empty_node_genisis(
            ip_address_c,
            port_c,
            vec![Peer::new(wallet_a.address, ip_address_a, port_a)],
            false,
            None,
        )
        .await;

        inner_test_new_node_online(
            &mut node_a,
            &mut node_b,
            &mut node_c,
            &mut wallet_a,
            &wallet_b,
            &wallet_c,
        )
        .await;
        // now, let A go offline. C will submit a transaction.
        println!("Stopping node A - expect ConnetionRefused errors");
        node_a.stop().await;
        // pause a sec - TODO remove this after fixing the join on stoping
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        // now, C will submit a transaction to B
        let _ = submit_transaction(
            &mut node_c,
            &mut wallet_c,
            wallet_b.address,
            0,
            false,
            None,
        )
        .await
        .unwrap();
        // pause a sec
        tokio::time::sleep(std::time::Duration::from_secs(15)).await; // time to settle, and give up waiting for full block
        // now both shold have 3 blocks
        let chain_b = node_b.inner.chain.lock().await;
        assert!(chain_b.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_b.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_b);
        // now c
        let chain_c = node_c.inner.chain.lock().await;
        assert!(chain_c.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_c.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_c);
        // put A back online
        println!("Restarting node A");
        node_a.serve().await;
        assert!(node_a.inner.state.lock().await.clone() == NodeState::ChainSyncing);
        // wait for a bit
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // check if node a knows about node b and c
        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&wallet_b.address));
        assert!(peers_a.contains_key(&wallet_c.address));
        drop(peers_a);
        // now check that it collected the block
        let chain_a = node_a.inner.chain.lock().await;
        assert_eq!(chain_a.as_ref().unwrap().depth, 2); // 3 blocks
        assert_eq!(chain_a.as_ref().unwrap().blocks.len(), 3); // 3 blocks
        // now, node A should be in serving state
        assert!(node_a.inner.state.lock().await.clone() == NodeState::Serving);
    }

    #[tokio::test]
    async fn test_sync_two_blocks() {
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 8));
        let port_a = 7999;

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 9));
        let port_b = 7998;

        let ip_address_c = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 5));
        let port_c = 7997;


        let (mut node_b, wallet_b) = create_empty_node_genisis(
            ip_address_b,
            port_b,
            vec![],
            true,
            Some(MinerPool::new()),
        )
        .await;

        let (mut node_a, mut wallet_a) = create_empty_node_genisis(
            ip_address_a,
            port_a,
            vec![Peer::new(wallet_b.address, ip_address_b, port_b)],
            true,
            None,
        )
        .await;

        let (mut node_c, mut wallet_c) = create_empty_node_genisis(
            ip_address_c,
            port_c,
            vec![Peer::new(wallet_a.address, ip_address_a, port_a)],
            false,
            None,
        )
        .await;

        inner_test_new_node_online(
            &mut node_a,
            &mut node_b,
            &mut node_c,
            &mut wallet_a,
            &wallet_b,
            &wallet_c,
        )
        .await;
        // now, let A go offline. C will submit a transaction.
        println!("Stopping node A - expect ConnetionRefused errors");
        node_a.stop().await;
        // pause a sec - TODO remove this after fixing the join on stoping
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        // now, C will submit a transaction to B
        let _ = submit_transaction(
            &mut node_c,
            &mut wallet_c,
            wallet_b.address,
            0,
            false,
            None,
        )
        .await
        .unwrap();
        // pause a sec
        tokio::time::sleep(std::time::Duration::from_secs(15)).await; // time to settle, and give up waiting for full block
        // now both shold have 3 blocks
        let chain_b = node_b.inner.chain.lock().await;
        assert!(chain_b.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_b.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_b);
        // now c
        let chain_c = node_c.inner.chain.lock().await;
        assert!(chain_c.as_ref().unwrap().depth == 2); // 3 blocks
        assert!(chain_c.as_ref().unwrap().blocks.len() == 3); // 3 blocks
        drop(chain_c);
        // DO A SECOND TRANSACTION TO GET 2 BLOCKS OUTDATED
        let _ = submit_transaction(
            &mut node_c,
            &mut wallet_c,
            wallet_b.address,
            0,
            false,
            None,
        )
        .await
        .unwrap();
        // pause a sec
        tokio::time::sleep(std::time::Duration::from_secs(15)).await; // time to settle, and give up waiting for full block
        // now both shold have 4 blocks
        let chain_b = node_b.inner.chain.lock().await;
        assert!(chain_b.as_ref().unwrap().depth == 3); // 4 blocks
        assert!(chain_b.as_ref().unwrap().blocks.len() == 4); // 4 blocks
        drop(chain_b);
        // now c
        let chain_c = node_c.inner.chain.lock().await;
        assert!(chain_c.as_ref().unwrap().depth == 3); // 4 blocks
        assert!(chain_c.as_ref().unwrap().blocks.len() == 4); // 4 blocks
        drop(chain_c);
        // put A back online
        println!("Restarting node A");
        node_a.serve().await;
        assert!(node_a.inner.state.lock().await.clone() == NodeState::ChainSyncing);
        // wait for a bit
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // check if node a knows about node b and c
        let peers_a = node_a.inner.peers.lock().await;
        assert!(peers_a.contains_key(&wallet_b.address));
        assert!(peers_a.contains_key(&wallet_c.address));
        drop(peers_a);
        // now check that it collected the block
        let chain_a = node_a.inner.chain.lock().await;
        assert_eq!(chain_a.as_ref().unwrap().depth, 3); // 3 blocks
        assert_eq!(chain_a.as_ref().unwrap().blocks.len(), 4); // 3 blocks
        // now, node A should be in serving state
        assert!(node_a.inner.state.lock().await.clone() == NodeState::Serving);
    }

    #[tokio::test]
    async fn test_multiple_transaction_one_block() {
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 8));
        let port_a = 7997;

        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 9));
        let port_b = 7996;

        let (node_b, wallet_b) = create_empty_node_genisis(
            ip_address_b,
            port_b,
            vec![],
            true,
            Some(MinerPool::new()),
        )
        .await;

        let (mut node_a, mut wallet_a) = create_empty_node_genisis(
            ip_address_a,
            port_a,
            vec![Peer::new(wallet_b.address, ip_address_b, port_b)],
            true,
            None,
        )
        .await;

        let mut miner_b = Miner::new(node_b.clone()).unwrap();

        miner_b.serve().await;
        node_a.serve().await;

        tokio::time::sleep(std::time::Duration::from_secs(2)).await; // wait for the nodes to connect
        // check states
        assert!(node_a.inner.state.lock().await.clone() == NodeState::Serving);
        assert!(node_b.inner.state.lock().await.clone() == NodeState::Serving);
        let state_manager = node_a
            .inner
            .chain
            .lock()
            .await
            .as_mut()
            .unwrap()
            .state_manager
            .clone();
        println!("Submitting multiple transactions from A to B");
        submit_transaction(
            &mut node_a,
            &mut wallet_a,
            wallet_b.address,
            0,
            false,
            None,
        )
        .await
        .unwrap();
        println!("Submitting multiple transactions from A to B");
        submit_transaction(
            &mut node_a,
            &mut wallet_a,
            wallet_b.address,
            0,
            false,
            None,
        )
        .await
        .unwrap();
        println!("waiting on transactions to be processed");
        tokio::time::sleep(std::time::Duration::from_secs(
            2 * MAX_TRANSACTION_WAIT_TIME,
        ))
        .await; // wait for the transactions to be processed
        println!("transactions processed, checking the chain");
        // now, lets check that a is in bs peer list
        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&wallet_a.address));
        drop(peers_b);
        // now, grab the chains - check depth
        let chain_b = miner_b.node.inner.chain.lock().await;
        assert_eq!(chain_b.as_ref().unwrap().depth, 1); // 2 blocks
        assert_eq!(chain_b.as_ref().unwrap().blocks.len(), 2); // 3 blocks
        // check that the leaf has 2 transactions
        let chain_b = chain_b.as_ref().unwrap();
        let leaf = chain_b.blocks.get(&chain_b.deepest_hash).unwrap();
        assert_eq!(leaf.transactions.len(), 2); // 2 transactions
        // now, check the chain of a
        let chain_a = node_a.inner.chain.lock().await;
        assert_eq!(chain_a.as_ref().unwrap().depth, 1); // 3 blocks
        assert_eq!(chain_a.as_ref().unwrap().blocks.len(), 2); // 3 blocks
        // check that the leaf has 2 transactions
        let chain_a = chain_a.as_ref().unwrap();
        let leaf = chain_a.blocks.get(&chain_a.deepest_hash).unwrap();
        assert_eq!(leaf.transactions.len(), 2); // 2 transactions
    }

    #[tokio::test]
    async fn test_50_transactions(){
        // make 2 nodes
        let ip_address_a = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 10));
        let port_a = 8000;
        
        let ip_address_b = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 11));
        let port_b = 8001;
        
        let (node_b, mut wallet_b) = create_empty_node_genisis(
            ip_address_b,
            port_b,
            vec![],
            true,
            Some(MinerPool::new()),
        )
        .await;

        let (mut node_a, mut wallet_a) = create_empty_node_genisis(
            ip_address_a,
            port_a,
            vec![Peer::new(wallet_b.address, ip_address_b, port_b)],
            true,
            None,
        )
        .await;

        // start the nodes
        let mut miner_b = Miner::new(node_b.clone()).unwrap();
        miner_b.serve().await;
        node_a.serve().await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await; // wait for the nodes to connect
        // check states
        assert!(node_a.inner.state.lock().await.clone() == NodeState::Serving);
        assert!(node_b.inner.state.lock().await.clone() == NodeState::Serving);
        // submit 50 transactions - first 10 from node a to node b

        for i in 0..10 {
            println!("Submitting transaction {} from A to B", i);
            let _ = submit_transaction(
                &mut node_a,
                &mut wallet_a,
                wallet_b.address,
                0,
                false,
                None,
            )
            .await
            .unwrap();
        }
        // pause a sec, check htat there is a block with signatures
        // also make sure that node b has money
        // IF TEST CASE FAILS< CHECK IF THE N TRANSACTIONS TO MINE COMPLETION CHANGED
        tokio::time::sleep(std::time::Duration::from_secs(2)).await; // wait for the transactions to be processed
        // now, lets check that a is in bs peer list
        let peers_b = node_b.inner.peers.lock().await;
        assert!(peers_b.contains_key(&wallet_a.address));
        drop(peers_b);
        // now, grab the chains - check depth
        let chain_b_lock = miner_b.node.inner.chain.lock().await;
        let chain_b = chain_b_lock.as_ref().unwrap();
        assert_eq!(chain_b.depth, 1); // 2 blocks
        assert_eq!(chain_b.blocks.len(), 2); // 2 blocks
        // check that the leaf has 10 transactions
        let leaf = chain_b.blocks.get(&chain_b.deepest_hash).unwrap();
        assert_eq!(leaf.transactions.len(), 10); // 10 transactions
        drop(chain_b_lock);
        // now, check the chain of a
        let chain_a_lock = node_a.inner.chain.lock().await;
        let chain_a = chain_a_lock.as_ref().unwrap();
        assert_eq!(chain_a.depth, 1); // 2blocks
        assert_eq!(chain_a.blocks.len(), 2); // 2 blocks
        // check that the leaf has 10 transactions
        let leaf = chain_a.blocks.get(&chain_a.deepest_hash).unwrap();
        assert_eq!(leaf.transactions.len(), 10); // 10 transactions
        drop(chain_a_lock);
        // now, check the balance of b 
        let state_root = node_b
            .inner
            .chain
            .lock()
            .await
            .as_ref()
            .unwrap()
            .get_state_root()
            .unwrap();
        let account_b = node_b
            .inner
            .chain
            .lock()
            .await
            .as_mut()
            .unwrap()
            .state_manager
            .get_account(&wallet_b.address, state_root)
            .unwrap();
        let b_balance = account_b.balance;
        assert_eq!(b_balance, get_reward_from_depth_and_stampers(1, 2));
        drop(account_b);
        // do it on the other side 
        let state_root = node_a
            .inner
            .chain
            .lock()
            .await
            .as_ref()
            .unwrap()
            .get_state_root()
            .unwrap();
        let acc = node_a
            .inner
            .chain
            .lock()
            .await
            .as_mut()
            .unwrap()
            .state_manager
            .get_account(&wallet_b.address, state_root)
            .unwrap();
        let balance_b = acc.balance;
        assert_eq!(balance_b, get_reward_from_depth_and_stampers(1, 2));
        drop(acc);
        // now, b will send some of its money to a

        for i in 0..10 {
            println!("Submitting transaction {} from B to A", i);
            let _ = submit_transaction(
                &mut miner_b.node,
                &mut wallet_b,
                wallet_a.address,
                10,
                false,
                None,
            )
            .await
            .unwrap();
        }
        // pause a sec, check htat there is a block with signatures
        tokio::time::sleep(std::time::Duration::from_secs(2)).await; // wait for the transactions to be processed
        // now, check chain depth
        let chain_b_lock = miner_b.node.inner.chain.lock().await;
        let chain_b = chain_b_lock.as_ref().unwrap();
        assert_eq!(chain_b.depth, 2); // 3 blocks
        assert_eq!(chain_b.blocks.len(), 3); // 3 blocks
        // check that the leaf has 10 transactions
        let leaf = chain_b.blocks.get(&chain_b.deepest_hash).unwrap();
        assert_eq!(leaf.transactions.len(), 10); // 10 transactions
        drop(chain_b_lock);
        // now, check the chain of a
        let chain_a_lock = node_a.inner.chain.lock().await;
        let chain_a = chain_a_lock.as_ref().unwrap();
        assert_eq!(chain_a.depth, 2); // 3 blocks
        assert_eq!(chain_a.blocks.len(), 3); // 3 blocks
        // check that the leaf has 10 transactions
        let leaf = chain_a.blocks.get(&chain_a.deepest_hash).unwrap();
        assert_eq!(leaf.transactions.len(), 10); // 10 transactions
        drop(chain_a_lock);
        // now, check the balance of a
        let state_root = node_a
            .inner
            .chain
            .lock()
            .await
            .as_ref()
            .unwrap()
            .get_state_root()
            .unwrap();
        let account_a = node_a
            .inner
            .chain
            .lock()
            .await
            .as_mut()
            .unwrap()
            .state_manager
            .get_account(&wallet_a.address, state_root)
            .unwrap();
        let a_balance = account_a.balance;
        assert_eq!(a_balance, 100); // 100 is the initial balance
        drop(account_a);
        // do it on the other side
        let state_root = node_b
            .inner
            .chain
            .lock()
            .await
            .as_ref()
            .unwrap()
            .get_state_root()
            .unwrap();
        let acc = node_b
            .inner
            .chain
            .lock()
            .await
            .as_mut()
            .unwrap()
            .state_manager
            .get_account(&wallet_a.address, state_root)
            .unwrap();
        let balance_a = acc.balance;
        assert_eq!(balance_a, 100); // 100 is the initial balance
        drop(acc);
        // check balance b 
        let state_root = node_b
            .inner
            .chain
            .lock()
            .await
            .as_ref()
            .unwrap()
            .get_state_root()
            .unwrap();
        let account_b = node_b
            .inner
            .chain
            .lock()
            .await
            .as_mut()
            .unwrap()
            .state_manager
            .get_account(&wallet_b.address, state_root)
            .unwrap();
        let b_balance = account_b.balance;
        assert_eq!(b_balance, get_reward_from_depth_and_stampers(1, 2) + get_reward_from_depth_and_stampers(2, 2) - 100); // 0 is the balance after sending 10 * 10 = 100
        drop(account_b);
        // do it on the other side
        let state_root = node_a
            .inner
            .chain
            .lock()
            .await
            .as_ref()
            .unwrap()
            .get_state_root()
            .unwrap();
        let acc = node_a
            .inner
            .chain
            .lock()
            .await
            .as_mut()
            .unwrap()
            .state_manager
            .get_account(&wallet_b.address, state_root)
            .unwrap();
        let balance_b = acc.balance;
        assert_eq!(balance_b, get_reward_from_depth_and_stampers(1, 2) + get_reward_from_depth_and_stampers(2, 2) - 100); // 0 is the balance after sending 10 * 10 = 100
        drop(acc);

    }
}
