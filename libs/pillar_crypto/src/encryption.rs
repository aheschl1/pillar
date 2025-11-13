use chacha20poly1305::{AeadCore, ChaCha20Poly1305, Key, KeyInit, aead::Aead};
use hkdf::Hkdf;
use pillar_serialize::StdByteArray;
use rand_core::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
use sha3::Sha3_256;

use crate::types::STANDARD_ARRAY_LENGTH;


type X25519PublicKey = StdByteArray;

pub struct KeyPair {
    pub public_key: StdByteArray,
    pub private_key: EphemeralSecret,
}

fn generate_public_private_keypair() -> KeyPair {
    let private_key = EphemeralSecret::random_from_rng(&mut OsRng);
    let public_key = PublicKey::from(&private_key);
    KeyPair {
        public_key: *public_key.as_bytes(),
        private_key,
    }
}

enum SecretState{
    Initiator(EphemeralSecret),
    Joined(ChaCha20Poly1305),
    None // for uninitialized state, useful for mem replace
}

pub struct PillarSharedSecret{
    pub public: X25519PublicKey,
    state: SecretState,
}

impl PillarSharedSecret{

    fn derive_cipher(shared_secret: SharedSecret) -> Result<ChaCha20Poly1305, std::io::Error> {
        let raw_secret_bytes = shared_secret.as_bytes();
        let hk = Hkdf::<Sha3_256>::new(None, raw_secret_bytes);
        
        let mut key_bytes = [0u8; STANDARD_ARRAY_LENGTH];
        hk.expand(&[], &mut key_bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("HKDF error: {:?}", e)))?;
        
        let key = Key::from_slice(&key_bytes);
        Ok(ChaCha20Poly1305::new(key))
    }

    /// Initiates a key exchange, generating a new key pair
    /// In this case, hydrate needs to be called later with the remote public key
    pub fn initiate() -> Self {
        let keypair = generate_public_private_keypair();
        PillarSharedSecret{
            public: keypair.public_key,
            state: SecretState::Initiator(keypair.private_key),
        }
    }

    /// Joins an existing key exchange given the remote public key
    /// In this case, hydrate does not need to be called later
    pub fn join(remote_public: X25519PublicKey) -> Result<Self, std::io::Error> {
        let keypair = generate_public_private_keypair();
        let remote_public_key = x25519_dalek::PublicKey::from(remote_public);
        let shared_secret = keypair.private_key.diffie_hellman(&remote_public_key);
        
        let cipher = PillarSharedSecret::derive_cipher(shared_secret)?;

        Ok(PillarSharedSecret{
            public: keypair.public_key,
            state: SecretState::Joined(cipher),
        })
    }

    /// Given the remote public key, computes the shared secret
    pub fn hydrate(&mut self, remote_public: X25519PublicKey) -> Result<(), std::io::Error> {
        let remote_public_key = x25519_dalek::PublicKey::from(remote_public);
        match std::mem::replace(&mut self.state, SecretState::None) {
            SecretState::Initiator(private_key) => {
                let shared_secret = private_key.diffie_hellman(&remote_public_key);
                let cipher = PillarSharedSecret::derive_cipher(shared_secret)?;
                self.state = SecretState::Joined(cipher);
            },
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other, 
                    "Shared secret already established, or invalid state"
                ));
            },
        }
        Ok(())
    }

    pub fn encrypt(&self, plaintext: impl Into<Vec<u8>>) -> Result<Vec<u8>, std::io::Error> {
        let cipher = match &self.state {
            SecretState::Joined(c) => c,
            _ => return Err(std::io::Error::new(
                std::io::ErrorKind::Other, 
                "Cipher not established. Run join or hydrate first."
            )),
        };

        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng); // 12 bytes
        
        let ciphertext_with_tag = cipher.encrypt(&nonce, plaintext.into().as_ref())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        let mut full_payload = Vec::new();
        full_payload.extend_from_slice(nonce.as_slice());
        full_payload.extend_from_slice(&ciphertext_with_tag);

        Ok(full_payload)
    }

    pub fn decrypt(&self, ciphertext: impl Into<Vec<u8>>) -> Result<Vec<u8>, std::io::Error> {
        let cipher = match &self.state {
            SecretState::Joined(c) => c,
            _ => return Err(std::io::Error::new(
                std::io::ErrorKind::Other, 
                "Cipher not established. Run join or hydrate first."
            )),
        };

        let data = ciphertext.into();
        if data.len() < 12 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Ciphertext too short to contain nonce",
            ));
        }

        let (nonce_bytes, ciphertext_with_tag) = data.split_at(12);
        let nonce = chacha20poly1305::Nonce::from_slice(nonce_bytes);

        let plaintext = cipher.decrypt(nonce, ciphertext_with_tag)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        )?;

        Ok(plaintext)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let keypair = generate_public_private_keypair();
        assert_eq!(keypair.public_key.len(), 32);
    }

    #[test]
    fn test_shared_secret_encryption_decryption() {
        // Simulate Alice initiating and Bob joining
        let mut alice = PillarSharedSecret::initiate();
        let bob = PillarSharedSecret::join(alice.public.clone()).unwrap();

        // Alice hydrates his cipher with bob's public key
        alice.hydrate(bob.public.clone()).unwrap();

        assert!(matches!(alice.state, SecretState::Joined(_)));
        assert!(matches!(bob.state, SecretState::Joined(_)));

        let message = b"Hello Pillar!";
        let ciphertext = alice.encrypt(message).unwrap();
        println!("Ciphertext: {:?}", ciphertext);
        let decrypted = bob.decrypt(ciphertext).unwrap();

        assert_eq!(decrypted, message);
    }

    #[test]
    fn test_encrypt_without_cipher() {
        let secret = PillarSharedSecret::initiate();
        let result = secret.encrypt(b"test");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_without_cipher() {
        let secret = PillarSharedSecret::initiate();
        let result = secret.decrypt(b"test");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_invalid_ciphertext() {
        let mut secret = PillarSharedSecret::initiate();
        let keypair = generate_public_private_keypair();
        let remote_key = keypair.public_key;
        secret.hydrate(remote_key).unwrap();

        let bad_ciphertext = vec![0u8; 10]; // too short
        let result = secret.decrypt(bad_ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_hydrate_without_private_key() {
        let mut secret = PillarSharedSecret{
            public: [0u8; 32],
            state: SecretState::None,
        };
        let result = secret.hydrate([1u8; 32]);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_hydrate_calls() {
        let mut alice = PillarSharedSecret::initiate();
        let bob = PillarSharedSecret::join(alice.public.clone()).unwrap();

        // First hydrate should work
        assert!(alice.hydrate(bob.public.clone()).is_ok());

        // Second hydrate should fail since private key is consumed
        assert!(alice.hydrate(bob.public.clone()).is_err());
    }

    #[test]
    fn test_long_message_encryption_decryption() {
        let mut alice = PillarSharedSecret::initiate();
        let bob = PillarSharedSecret::join(alice.public.clone()).unwrap();
        alice.hydrate(bob.public.clone()).unwrap();

        let long_message = vec![0u8; 10_000]; // 10 KB of zeros
        let ciphertext = alice.encrypt(long_message.clone()).unwrap();
        let decrypted = bob.decrypt(ciphertext).unwrap();

        assert_eq!(decrypted, long_message);
    }

    #[test]
    fn test_multiple_use_of_cipher() {
        let mut alice = PillarSharedSecret::initiate();
        let bob = PillarSharedSecret::join(alice.public.clone()).unwrap();
        alice.hydrate(bob.public.clone()).unwrap();

        let message1 = b"First message";
        let ciphertext1 = alice.encrypt(message1).unwrap();
        let decrypted1 = bob.decrypt(ciphertext1).unwrap();
        assert_eq!(decrypted1, message1);

        let message2 = b"Second message";
        let ciphertext2 = alice.encrypt(message2).unwrap();
        let decrypted2 = bob.decrypt(ciphertext2).unwrap();
        assert_eq!(decrypted2, message2);
    }
}
