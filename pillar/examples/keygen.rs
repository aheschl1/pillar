//! Generate a wallet file a node can load, and print its address.
//!
//!     cargo run --example keygen -- <path/to/wallet.bin>
//!
//! A node loads `<work-dir>/<name>/wallet.bin`, so placing the file there before the
//! first start makes the node run as that wallet.
use pillar_core::{accounting::wallet::Wallet, persistence::Persistable};
use pillar_crypto::signing::SigFunction;

#[tokio::main]
async fn main() {
    let path = std::env::args().nth(1).expect("usage: keygen <path/to/wallet.bin>");
    let path = std::path::PathBuf::from(path);
    if path.exists() {
        eprintln!("refusing to overwrite {}", path.display());
        std::process::exit(1);
    }
    let wallet = Wallet::generate_random();
    wallet.save(&path).await.expect("failed to write wallet");
    println!("{}", hex::encode(wallet.address));
}
