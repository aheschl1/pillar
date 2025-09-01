//! Wallet wrapper around an ed25519 keypair with convenience methods.
use pillar_crypto::{signing::{DefaultSigner, DefaultVerifier, SigFunction, SigVerFunction, Signable}, types::StdByteArray};

/// A local wallet that can sign data and exposes its public address.
pub struct Wallet{
    pub address: StdByteArray,
    signing_key: DefaultSigner,
    _balance: u64,
    pub nonce: u64,
}

impl Wallet {
    /// Construct a wallet from a public address and private signing key.
    pub fn new(address: StdByteArray, signing_key: DefaultSigner) -> Self {
        Wallet {
            address,
            signing_key,
            _balance: 0,
            nonce: 0,
        }
    }

    /// Return the private key bytes (32) of this wallet.
    pub fn get_private_key(&self) -> [u8; 32] {
        self.to_bytes()
    }
}

impl SigFunction<32, 32, 64> for Wallet {
    
    fn to_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
    
    fn get_verifying_function(&self) -> impl SigVerFunction<32, 64> {
        DefaultVerifier::new(self.address)
    }
    
    /// Generate a new random wallet using a freshly generated ed25519 keypair.
    fn generate_random() -> Self {
        let signer = DefaultSigner::generate_random();
        let public_key = signer.get_verifying_function().to_bytes();
        Wallet {
            address: public_key,
            signing_key: signer,
            _balance: 0,
            nonce: 0,
        }
    }
    
    /// Sign the canonical bytes provided by `data` with the wallet's private key.
    fn sign(&mut self, data: &impl Signable<64>) -> [u8; 64] {
        self.signing_key.sign(data)
    }
}