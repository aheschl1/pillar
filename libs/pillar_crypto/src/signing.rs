use ed25519::signature::SignerMut;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;

use crate::types::StdByteArray;


/// A trait for an object that can be signed and verified.
pub trait Signable<const S: usize>{
    fn get_signing_bytes(&self) -> impl AsRef<[u8]>;
    /// Behavior to be implemented by the object that will be signed
    fn sign<const K: usize, const P: usize>(&mut self, signing_function: &mut impl SigFunction<K, P, S>) -> [u8; S];
}

/// A trait for signing and verifying signatures.
/// 
/// # Generics
/// 
/// * `K` - The size of the private key in bytes.
/// * `P` - The size of the public key in bytes.
/// * `S` - The size of the signature in bytes.
pub trait SigFunction<const K: usize, const P: usize, const S: usize>{
    /// Signs the given data using the provided private key.
    ///
    /// # Arguments
    ///
    /// * `data` - The data to be signed.
    /// * `private_key` - The private key used for signing.
    ///
    /// # Returns
    ///
    /// * `Ok(StdByteArray)` containing the signature.
    /// * `Err(std::io::Error)` if signing fails.
    fn sign(&mut self, data: &impl Signable<S>) -> [u8; S];
    
    /// Retreive the byte representation of the signing function
    /// This often will be a private key
    /// 
    /// # Returns
    /// 
    /// [u8; _] containing the byte representation of the signing function
    fn to_bytes(&self) -> [u8; K];

    /// Get the function that will verify the signature
    ///
    /// # Returns
    /// 
    /// * An object that implements the `SigVerFunction` trait.
    fn get_verifying_function(&self) -> impl SigVerFunction<P, S>;

    fn generate_random() -> Self;
}

/// A trait for verifying signatures.
/// 
/// # Generics
/// 
/// * `K` - The size of the public key in bytes.
/// * `S` - The size of the signature in bytes.
pub trait SigVerFunction<const K: usize, const S: usize>{
    /// Verifies the given signature using the provided public key.
    ///
    /// # Arguments
    ///
    /// * `signature` - The signature to be verified.
    ///
    /// # Returns
    ///
    /// * `true` if the signature is valid, `false` otherwise.
    fn verify(&self, signature: &[u8; S], target: &impl Signable<S>) -> bool;

    fn to_bytes(&self) -> [u8; K];

    fn from_bytes(bytes: &[u8; K]) -> Self;
}

/// Default signer is the ed25519 signing function
pub struct DefaultSigner{
    private_key: SigningKey
}

/// Default verifier is the ed25519 verifying function
pub struct DefaultVerifier{
    public_key: VerifyingKey
}

impl DefaultVerifier{
    pub fn new(public_key: StdByteArray) -> Self{
        DefaultVerifier{
            public_key: VerifyingKey::from_bytes(&public_key).expect("Invlaid public key")
        }
    }
}

impl DefaultSigner{
    pub fn new(private_key: StdByteArray) -> Self{
        DefaultSigner{
            private_key: SigningKey::from_bytes(&private_key)
        }
    }
}

impl SigFunction<32,32, 64> for DefaultSigner{
    fn sign(&mut self, data: &impl Signable<64>) -> [u8; 64]{
        self.private_key.sign(data.get_signing_bytes().as_ref()).to_bytes()
    }

    fn to_bytes(&self) -> StdByteArray{
        self.private_key.to_bytes()
    }

    fn get_verifying_function(&self) -> impl SigVerFunction<32, 64>{
        DefaultVerifier::new(self.private_key.verifying_key().to_bytes())
    }

    fn generate_random() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        DefaultSigner {
            private_key: signing_key
        }
    }
}

impl SigVerFunction<32, 64> for DefaultVerifier{
    fn verify(&self, signature: &[u8; 64], target: &impl Signable<64>) -> bool{
        let signature = ed25519::Signature::from_bytes(signature);

        self.public_key.verify_strict(target.get_signing_bytes().as_ref(), &signature).is_ok()
    }

    fn to_bytes(&self) -> StdByteArray{
        self.public_key.to_bytes()
    }

    fn from_bytes(bytes: &StdByteArray) -> Self{
        DefaultVerifier::new(*bytes)
    }
}