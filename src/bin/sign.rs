use base64::{engine::Engine, prelude::BASE64_URL_SAFE};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};

fn main() {
    let ramdomness: [u8; 32] = [9; 32];

    // 1. Generate a new keypair
    let signing_key = SigningKey::from_bytes(&ramdomness);
    let verifying_key = signing_key.verifying_key();

    // 2. Export to Base64 strings
    let sk_b64 = BASE64_URL_SAFE.encode(signing_key.to_bytes()); // 32-byte private key
    let vk_b64 = BASE64_URL_SAFE.encode(verifying_key.to_bytes()); // 32-byte public key
    println!("Private Key (Base64): {}", sk_b64);
    println!("Public Key  (Base64): {}", vk_b64);

    // 3. Import back from Base64
    let sk_bytes = BASE64_URL_SAFE.decode(&sk_b64).unwrap();
    let vk_bytes = BASE64_URL_SAFE.decode(&vk_b64).unwrap();

    let sk2 = SigningKey::from_bytes(&sk_bytes.try_into().unwrap());
    let vk2 = VerifyingKey::from_bytes(&vk_bytes.try_into().unwrap()).unwrap();

    println!("✅ Key export/import works!");

    // 4. Test signing/verification
    let message = [234u8; 1450];
    let sig = sk2.sign(&message);
    println!("{:?}", sig.to_bytes());
    assert!(vk2.verify(&message, &sig).is_ok());
}
