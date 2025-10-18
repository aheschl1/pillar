//! Wallet wrapper around an ed25519 keypair with convenience methods.
use pillar_crypto::{signing::{DefaultSigner, DefaultVerifier, SigFunction, SigVerFunction, Signable}, types::{StdByteArray, STANDARD_ARRAY_LENGTH}};
use pillar_serialize::PillarSerialize;

/// A local wallet that can sign data and exposes its public address.
#[derive(Clone)]
pub struct Wallet{
    pub address: StdByteArray,
    signing_key: DefaultSigner,
    _balance: u64,
    nonce: u64,
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

    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    pub fn nonce_mut(&mut self) -> &mut u64 {
        &mut self.nonce
    }
}

impl PillarSerialize for Wallet {
    fn serialize_pillar(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut data = Vec::new();
        data.extend(self.address.serialize_pillar()?);
        data.extend(self.signing_key.to_bytes().serialize_pillar()?);
        data.extend(self._balance.to_le_bytes());
        data.extend(self.nonce.to_le_bytes());
        Ok(data)
    }

    fn deserialize_pillar(data: &[u8]) -> Result<Self, std::io::Error> {
        let address = StdByteArray::deserialize_pillar(&data[0..STANDARD_ARRAY_LENGTH])?;
        let signing_key = DefaultSigner::new(
            StdByteArray::deserialize_pillar(&data[STANDARD_ARRAY_LENGTH..2*STANDARD_ARRAY_LENGTH])?
        );
        let balance = u64::from_le_bytes(data[2*STANDARD_ARRAY_LENGTH..2*STANDARD_ARRAY_LENGTH+8].try_into().unwrap());
        let nonce = u64::from_le_bytes(data[2*STANDARD_ARRAY_LENGTH+8..].try_into().unwrap());
        Ok(Wallet {
            address,
            signing_key,
            _balance: balance,
            nonce,
        })
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