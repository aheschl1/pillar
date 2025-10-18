use std::{net::IpAddr, path::PathBuf};

use pillar_crypto::types::StdByteArray;

use crate::{accounting::wallet::Wallet, blockchain::chain::Chain, nodes::{node::Node, peer::{Peer, PillarIPAddr}}, persistence::Persistable};



pub struct PersistenceManager {
    pub root: PathBuf,
    chain_root: PathBuf,
    node_root: PathBuf,
}

pub(crate) struct NodeShard {
    pub ip_address: IpAddr,
    pub port: u16,
    pub peers: Vec<Peer>,
    pub public_key: StdByteArray,
    pub private_key: StdByteArray
}

impl PersistenceManager {
    /// Creates a new state manager working off 
    /// the root. 
    /// 
    /// If None, defaults to ~/.pillar
    pub fn new(root: Option<PathBuf>) -> Self {
        let root = match root {
            Some(a) => a,
            None => {
                tracing::info!("State setting up at ~/.pillar");
                let home = dirs::home_dir().expect("Cannot startup state without home dir if a path is not specified.");
                let r = home.join(".pillar");
                if ! std::fs::exists(&r).unwrap() {
                    tracing::info!("Creating folder {}", r.display());
                    std::fs::create_dir_all(&r).unwrap();
                }
                r
            }
        };
        if !std::fs::exists(root.join("chain")).unwrap() {
            std::fs::create_dir_all(root.join("chain")).unwrap();
        }
        if !std::fs::exists(root.join("node")).unwrap() {
            std::fs::create_dir_all(root.join("node")).unwrap();
        }
        Self {
            root: root.clone(),
            chain_root: root.clone().join("chain"),
            node_root: root.join("node"),
        }
    }

    pub async fn save_node(&self, node: &Node) -> Result<(), std::io::Error>{
        if let Some(chain) = node.inner.chain.lock().await.as_ref() {
            chain.save(&self.chain_root).await?;
        }

        let address: PillarIPAddr = node.ip_address.into();

        node.inner.public_key.save(&self.node_root.join("public_key.bin")).await?;
        node.inner.private_key.save(&self.node_root.join("private_key.bin")).await?;
        node.inner.peers.read()
            .await.values()
            .cloned()
            .collect::<Vec<Peer>>()
            .save(&self.node_root.join("peers.bin")).await?;
        address.save(&self.node_root.join("ip_address.bin")).await?;
        node.port.save(&self.node_root.join("port.bin")).await?;

        Ok(())
    }

    pub(crate) async fn load_node(&self) -> Result<NodeShard, std::io::Error> {
        let public_key = StdByteArray::load(&self.node_root.join("public_key.bin")).await?;
        let private_key = StdByteArray::load(&self.node_root.join("private_key.bin")).await?;
        let peers = Vec::<Peer>::load(&self.node_root.join("peers.bin")).await?;
        let ip_address: PillarIPAddr = PillarIPAddr::load(&self.node_root.join("ip_address.bin")).await?;
        let port = u16::load(&self.node_root.join("port.bin")).await?;

        Ok(NodeShard {
            ip_address: ip_address.into(),
            port,
            peers,
            public_key,
            private_key
        })

    }

    pub async fn load_chain(&self) -> Result<Option<Chain>, std::io::Error> {
        if std::fs::metadata(self.chain_root.join("meta.bin")).is_ok() {
            Ok(Some(Chain::load(&self.chain_root).await?))
        } else {
            Ok(None)
        }
    }

    pub fn has_node(&self) -> bool {
        std::fs::metadata(self.node_root.join("private_key.bin")).is_ok()
    }

    pub async fn save_wallet(&self, wallet: &Wallet) -> Result<(), std::io::Error> {
        wallet.save(&self.root.join("wallet.bin")).await
    }

    pub async fn load_wallet(&self) -> Result<Option<Wallet>, std::io::Error> {
        if std::fs::metadata(self.root.join("wallet.bin")).is_err() {
            return Ok(None);
        }
        Ok(Some(Wallet::load(&self.root.join("wallet.bin")).await?))
    }

}