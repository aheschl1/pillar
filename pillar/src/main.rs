
mod run;
mod ws_handles;

use pillar_core::{accounting::wallet::Wallet, nodes::peer::Peer};
use pillar_crypto::signing::SigFunction;
use pillar_serialize::PillarSerialize;
use clap::Parser;
use tracing_subscriber::{filter::LevelFilter, fmt::{self, writer::BoxMakeWriter}, layer::SubscriberExt, util::SubscriberInitExt, Layer, Registry};
use std::{fs::File, path::PathBuf};

use crate::run::launch_node;


#[derive(Clone)]
struct Config {
    /// Well-known peers to connect to on startup
    wkps: Vec<Peer>,
    ip_address: std::net::IpAddr,
    wallet: Wallet
}

impl Config {
    fn new(wkps: Vec<Peer>, ip_address: std::net::IpAddr, wallet: Option<Wallet>) -> Self {
        let wallet = if let Some(w) = wallet {
            w
        } else {
            Wallet::generate_random()
        };
        Config { wkps, ip_address, wallet }
    }

    fn from_root(root: &PathBuf) -> Self {
        let path = root.join("config.bin");
        Self::from_file(&path)
    }

    fn from_file(path: &PathBuf) -> Self {
        let data = std::fs::read(path).expect("failed to read config file");
        Self::deserialize_pillar(&data).expect("failed to deserialize config file")
    }

    fn save(&self, root: &PathBuf) {
        let path = root.join("config.bin");
        let data = self.serialize_pillar().expect("failed to serialize config");
        std::fs::write(path, data).expect("failed to write config file");
    }
}

impl PillarSerialize for Config {
    // TODO optimize this serialization. Wallet is a fixed size, so we don't need to store its length
    // Same for IP address, we can store it as 4 or 16 bytes depending on if it's IPv4 or IPv6
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut data = Vec::new();
        let wkps_ser = self.wkps.serialize_pillar()?;
        data.extend((wkps_ser.len() as u32).to_le_bytes());
        data.extend(wkps_ser);
        let wallet_ser = self.wallet.serialize_pillar()?;
        data.extend((wallet_ser.len() as u32).to_le_bytes());
        data.extend(wallet_ser);
        data.extend(self.ip_address.to_string().serialize_pillar()?);
        Ok(data)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let wkps_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        let wkps = Vec::<Peer>::deserialize_pillar(&data[4..4+wkps_len])?;
        let wallet_len = u32::from_le_bytes(data[4+wkps_len..8+wkps_len].try_into().unwrap()) as usize;
        let wallet = Wallet::deserialize_pillar(&data[8+wkps_len..8+wkps_len+wallet_len])?;
        let ip_address = String::deserialize_pillar(&data[8+wkps_len+wallet_len..])?;
        Ok(Config { wkps, ip_address: ip_address.parse().unwrap(), wallet })
    }
}

fn setup_tracing(log_dir: &PathBuf, run_name: String) {
    let log_dir = log_dir.to_str().unwrap();
    std::fs::create_dir_all(log_dir).expect("failed to create log directory");

    let filename = format!("{log_dir}/output.log");
    let file = File::create(filename).expect("failed to create log file");

    // === Console output for WARN and above ===
    let console_layer = fmt::layer()
        .with_ansi(true)
        .with_level(true)
        .with_filter(LevelFilter::INFO);

    let file_layer = fmt::layer()
        .with_writer(BoxMakeWriter::new(file))
        .with_ansi(false)
        .with_level(true)
        .with_filter(LevelFilter::DEBUG);

    // === Combined subscriber ===
    Registry::default()
        .with(file_layer)
        .with(console_layer)
        .try_init()
        .expect("Failed to initialize tracing subscriber");
}

#[derive(Parser, Debug)]
#[command(version, about = "Pillar Node")]
struct Args {
    #[arg(short, long, help = "Root directory for output files", default_value = "./work")]
    work_dir: PathBuf,
    #[arg(short, long, help = "IP address to bind the node to", default_value = "0.0.0.0")]
    ip_address: String,
    #[arg(short, long, help = "List of well-known peers in the format <ip>", num_args = 0.., value_delimiter = ',')]
    wkps: Vec<String>,
    #[arg(short, long, help = "Name of the run (folders will be created with this name)")]
    name: Option<String>,
    #[arg(short, long, help = "Path to config binary file (if not provided, a new wallet will be generated)")]
    config: Option<PathBuf>,
}


#[tokio::main]
async fn main() -> Result<(), ()> {
    let args = Args::parse();
    // create a run id
    let run_id = args.name.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    tracing::debug!("Run ID: {}", run_id);
    let log_dir = args.work_dir.join(run_id.clone());
    tracing::info!("Starting pillar node with output root: {}", args.work_dir.display());
    // setup a tracing subscriber
    setup_tracing(&log_dir, run_id);
    let ip_address = args.ip_address.clone();
    // sanitize ip address
    let ip_address = ip_address.parse::<std::net::IpAddr>().unwrap();
    // ensure not multicast
    if ip_address.is_multicast() {
        tracing::error!("Cannot bind to broadcast or multicast address");
        return Err(());
    }
    let wkps: Vec<Peer> = args.wkps.iter().filter_map(|ip| {
        let ipaddr = ip.parse::<std::net::IpAddr>();
        match ipaddr {
            Err(e) => {
                tracing::error!("Invalid IP address in well-known peers: {}", e);
                None
            },
            Ok(addr) => {
                Some(Peer::new([0u8; 32], addr, pillar_core::PROTOCOL_PORT)) // public key will be discovered later
            }
        }
    }).collect();
    let config = if let Some(config_path) = args.config {
        Config::from_file(&config_path)
    }else{
        let config = Config::new(wkps, ip_address, None);
        config.save(&log_dir);
        config
    };
    launch_node(config).await;
    Ok(())
}
