
use chrono::Local;
use pillar_core::nodes::peer::Peer;
use pillar_serialize::PillarSerialize;
use clap::Parser;
use tracing_subscriber::{filter::LevelFilter, fmt::{self, writer::BoxMakeWriter}, layer::SubscriberExt, util::SubscriberInitExt, Layer, Registry};
use std::{fs::File, path::PathBuf};

use crate::run::launch_node;
mod run;

struct Config {
    /// Well-known peers to connect to on startup
    wkps: Vec<Peer>,
    ip_address: std::net::IpAddr,
}

impl Config {
    fn new(wkps: Vec<Peer>, ip_address: std::net::IpAddr) -> Self {
        Config { wkps, ip_address }
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
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut data = Vec::new();
        let wkps_ser = self.wkps.serialize_pillar()?;
        data.extend((wkps_ser.len() as u32).to_le_bytes());
        data.extend(wkps_ser);
        data.extend(self.ip_address.to_string().serialize_pillar()?);
        Ok(data)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let wkps_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        let wkps = Vec::<Peer>::deserialize_pillar(&data[4..4+wkps_len])?;
        let ip_address = String::deserialize_pillar(&data[4+wkps_len..])?;
        Ok(Config { wkps, ip_address: ip_address.parse().unwrap()})
    }
}

fn setup_tracing(root: &PathBuf, run_id: uuid::Uuid) {
    let log_dir = root.join(run_id.to_string());
    let log_dir = log_dir.to_str().unwrap();
    std::fs::create_dir_all(log_dir).expect("failed to create log directory");

    let filename = format!("{log_dir}/output.log");
    let file = File::create(filename).expect("failed to create log file");

    // === Console output for WARN and above ===
    let console_layer = fmt::layer()
        .with_ansi(true)
        .with_level(true)
        .with_filter(LevelFilter::WARN);

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
    #[arg(short, long, help = "Root directory for output files")]
    root: PathBuf,
    #[arg(short, long, help = "IP address to bind the node to", default_value = "127.0.0.1")]
    ip_address: String,
}


#[tokio::main]
async fn main() {
    let args = Args::parse();
    println!("Starting pillar node with output root: {}", args.root.display());
    // create a run id
    let run_id = uuid::Uuid::new_v4();
    println!("Run ID: {}", run_id);
    // setup a tracing subscriber
    setup_tracing(&args.root, run_id);
    let ip_address = args.ip_address.clone();
    // sanitize ip
    let ip_address = ip_address.parse::<std::net::IpAddr>().unwrap();
    // ensure not multicast
    if ip_address.is_multicast() {
        panic!("Cannot bind to broadcast or multicast address");
    }
    let config = Config::new(vec![], ip_address);
    // save config
    config.save(&args.root);
    launch_node(&config).await;
}
