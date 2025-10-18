
mod run;
mod ws_handles;
mod log_stream;

use pillar_core::{nodes::peer::Peer};
use clap::Parser;
use tracing_subscriber::{filter::LevelFilter, fmt::{self, writer::BoxMakeWriter}, layer::SubscriberExt, util::SubscriberInitExt, Layer, Registry};
use std::{fs::File, path::PathBuf};

use crate::{log_stream::BroadcastWriter, run::launch_node};

fn setup_tracing(log_dir: &PathBuf) {
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

    let ws_layer = fmt::layer()
        .with_writer(BoxMakeWriter::new(BroadcastWriter))
        .with_filter(LevelFilter::INFO);

    // === Combined subscriber ===
    Registry::default()
        .with(file_layer)
        .with(console_layer)
        .with(ws_layer)
        .try_init()
        .expect("Failed to initialize tracing subscriber");
}

#[derive(Parser, Debug)]
#[command(version, about = "Pillar Node")]
struct Args {
    #[arg(short, long, help = "Root directory for output files", default_value = "./work")]
    work_dir: PathBuf,
    #[arg(short, long, help = "IP address to bind the node to", required = true)]
    ip_address: String,
    #[arg(long, help = "List of well-known peers in the form of <ipa>,<ipb>,...", num_args = 0.., value_delimiter = ',')]
    wkps: Vec<String>,
    #[arg(short, long, help = "Name of the run (folders will be created with this name)")]
    name: Option<String>,
    #[arg(short, long, help = "Path to config binary file (if not provided, a new wallet will be generated)")]
    config: Option<PathBuf>,
    #[arg(long, help = "Start with genesis block (only for testing, do not use in production)")]
    genesis: bool,
    #[arg(long, help = "Start node as a miner")]
    miner: bool,
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
    setup_tracing(&log_dir);
    let ip_address = args.ip_address.clone();
    // sanitize ip address
    let ip_address = ip_address.parse::<std::net::IpAddr>().unwrap();
    // ensure not multicast
    if ip_address.is_multicast() || ip_address.is_unspecified() {
        tracing::error!("Cannot bind to broadcast or multicast address, or unspecified address.");
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
    launch_node(
        args.genesis,
        args.miner,
        wkps,
        ip_address,
        log_dir
    ).await;
    Ok(())
}
