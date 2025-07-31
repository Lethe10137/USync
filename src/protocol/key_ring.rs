use blake3::KEY_LEN;
use ed25519_dalek::ed25519::signature::Signer;
use ed25519_dalek::{PUBLIC_KEY_LENGTH, Signature, SigningKey, VerifyingKey};

use std::collections::HashSet;
use std::sync::OnceLock;

pub static KEY_RING: OnceLock<KeyRing> = OnceLock::new();

#[derive(Debug, Default)]
pub struct KeyRing {
    pub public_key_rings: HashSet<VerifyingKey>,
    private_key: Option<SigningKey>,
}

fn prase_key(key: &String) -> Option<[u8; KEY_LEN]> {
    let mut buffer = [0u8; KEY_LEN];
    hex::decode_to_slice(key, &mut buffer).ok()?;
    buffer.into()
}

impl KeyRing {
    pub fn new(public_keys: Vec<String>, private_key: Option<String>) -> Self {
        let public_key_rings = public_keys
            .iter()
            .map(|key| {
                prase_key(key)
                    .as_ref()
                    .map(VerifyingKey::from_bytes)
                    .unwrap_or_else(|| panic!("{key} is not a 256-bit Hex number."))
                    .unwrap_or_else(|_| panic!("{key} is not a valid Verifying key"))
            })
            .collect();

        let private_key = private_key.as_ref().map(|key| {
            prase_key(key)
                .as_ref()
                .map(SigningKey::from_bytes)
                .unwrap_or_else(|| panic!("{key} is not a 256-bit Hex number."))
        });
        Self {
            public_key_rings,
            private_key,
        }
    }
    pub fn add_public_key(mut self, key: VerifyingKey) -> Self {
        self.public_key_rings.insert(key);
        self
    }
    pub fn set_private_key(mut self, key: SigningKey) -> Self {
        self.private_key = Some(key);
        self
    }
    pub fn sign_with_private_key(&self, content: &[u8]) -> Option<Signature> {
        self.private_key.as_ref().map(|key| key.sign(content))
    }

    pub fn derive_public_key(&self) -> Option<[u8; PUBLIC_KEY_LENGTH]> {
        self.private_key
            .as_ref()
            .map(|key| key.verifying_key().to_bytes())
    }
}

// Panic on second call!
pub fn init(public_keys: Vec<String>, private_key: Option<String>) {
    KEY_RING
        .set(KeyRing::new(public_keys, private_key))
        .expect("Second call of initialize");
}
