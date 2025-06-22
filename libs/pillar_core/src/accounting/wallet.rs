use pillar_crypto::{signing::{DefaultSigner, SigFunction}, types::{StdByteArray, STANDARD_ARRAY_LENGTH}};

pub struct Wallet{
    pub address: StdByteArray,
    pub signing_key: DefaultSigner,
    _balance: u64,
    pub nonce: u64,
}