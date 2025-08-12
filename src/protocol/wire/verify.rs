use std::ops::Deref;

use bytes::Bytes;
use crc::{CRC_64_ECMA_182, Crc, Digest};
use ed25519_dalek::{Signature, VerifyingKey};

use crate::protocol::key_ring::KeyRing;

use crate::constants::MTU;
pub fn check_crc64(content: &[u8]) -> u64 {
    Crc::<u64>::new(&CRC_64_ECMA_182).checksum(content)
}

pub fn hash_slices<H, O, B, T>(
    slices: T,
    mut hasher: H,
    mut update: impl FnMut(&mut H, B),
    finalize: impl FnOnce(H) -> O,
) -> O
where
    B: Deref<Target = [u8]>,
    T: IntoIterator<Item = B>,
    O: Sized,
    H: Sized,
{
    for slice in slices {
        update(&mut hasher, slice);
    }
    finalize(hasher)
}
#[derive(Clone)]
pub enum PacketVerificationData<'a> {
    CRC64 {
        pkt: &'a [u8],
        crc64: &'a [u8],
    },
    Ed25519 {
        pkt: &'a [u8],
        pub_key: &'a [u8],
        signature: &'a [u8],
    },
}

pub enum PacketVerifyType {
    CRC64,
    Ed25519,
}

impl<'a> PacketVerificationData<'a> {
    pub fn pkt_len(&self) -> usize {
        match self {
            Self::CRC64 { pkt, .. } => pkt.len(),
            Self::Ed25519 { pkt, .. } => pkt.len(),
        }
    }
}

#[derive(Debug)]
pub enum PacketVerificationError {
    IncorrectLength,
    PacketTooLong,
    UnknownPublicKey,
    CorruptContent,
    IncorrectSign,
}

impl KeyRing {
    pub fn sign<T, B>(&self, verification_type: PacketVerifyType, pkt: T) -> Bytes
    where
        T: IntoIterator<Item = B>,
        B: Deref<Target = [u8]>,
    {
        match verification_type {
            PacketVerifyType::CRC64 => {
                let hash = hash_slices(
                    pkt,
                    Crc::<u64>::new(&CRC_64_ECMA_182).digest(),
                    |digest, slice| Digest::<'_, u64, _>::update(digest, &slice),
                    Digest::<'_, u64, _>::finalize,
                )
                .to_be_bytes();
                Bytes::copy_from_slice(&hash)
            }

            PacketVerifyType::Ed25519 => {
                let hash = hash_slices(
                    pkt,
                    blake3::Hasher::new(),
                    |hasher, slice| {
                        blake3::Hasher::update(hasher, &slice);
                    },
                    |hasher| blake3::Hasher::finalize(&hasher),
                );

                let signature = self
                    .sign_with_private_key(hash.as_bytes())
                    .expect("No private key");

                Bytes::copy_from_slice(&signature.to_bytes())
            }
        }
    }

    fn verify_ed25519(
        &self,
        pkt: &[u8],
        pub_key: &[u8],
        signature: &[u8],
    ) -> Result<(), PacketVerificationError> {
        let verifying_key = self.parse_and_check_key(pub_key)?;
        let signature =
            Signature::try_from(signature).map_err(|_| PacketVerificationError::IncorrectLength)?;
        verifying_key
            .verify_strict(blake3::hash(pkt).as_bytes(), &signature)
            .map_err(|_| PacketVerificationError::IncorrectSign)
    }

    fn parse_and_check_key(&self, pub_key: &[u8]) -> Result<VerifyingKey, PacketVerificationError> {
        let key = VerifyingKey::try_from(pub_key)
            .map_err(|_| PacketVerificationError::IncorrectLength)?;
        if !self.public_key_rings.contains(&key) {
            return Err(PacketVerificationError::UnknownPublicKey);
        }
        Ok(key)
    }

    fn verify_crc64(pkt: &[u8], crc64: &[u8]) -> Result<(), PacketVerificationError> {
        (u64::from_be_bytes(
            crc64
                .try_into()
                .map_err(|_| PacketVerificationError::IncorrectLength)?,
        ) == check_crc64(pkt))
        .then_some(())
        .ok_or(PacketVerificationError::CorruptContent)
    }

    pub fn verify<'a>(
        &self,
        data: PacketVerificationData<'a>,
    ) -> Result<(), PacketVerificationError> {
        if data.pkt_len() > MTU {
            return Err(PacketVerificationError::PacketTooLong);
        }

        match data {
            PacketVerificationData::CRC64 { pkt, crc64 } => Self::verify_crc64(pkt, crc64),
            PacketVerificationData::Ed25519 {
                pkt,
                pub_key,
                signature,
            } => self.verify_ed25519(pkt, pub_key, signature),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::key_ring::KeyRing;
    use bytes::BytesMut;
    use ed25519_dalek::{SECRET_KEY_LENGTH, SigningKey, VerifyingKey};
    use rand::{TryRngCore, rngs::OsRng};
    use zerocopy::IntoBytes;

    fn generate_ed25519_private_key() -> [u8; SECRET_KEY_LENGTH] {
        let mut buf = [0u8; SECRET_KEY_LENGTH];
        OsRng.try_fill_bytes(&mut buf).unwrap();
        buf
    }

    fn generate_ed25519_key_pair() -> (SigningKey, VerifyingKey) {
        let signing_key = SigningKey::from(generate_ed25519_private_key());
        let verifying_key = VerifyingKey::from(&signing_key);
        (signing_key, verifying_key)
    }

    fn generate_key_rings() -> (KeyRing, KeyRing) {
        let (signing_key, verifying_key) = generate_ed25519_key_pair();
        let exported_signing_key = hex::encode(signing_key.as_bytes());
        let exported_verifying_key = hex::encode(verifying_key.as_bytes());
        dbg!(&exported_signing_key, &exported_verifying_key);

        let server_keyring = KeyRing::new(vec![exported_verifying_key], None);
        let clietn_keyring = KeyRing::new(vec![], Some(exported_signing_key));
        (server_keyring, clietn_keyring)
    }

    #[test]
    fn test_exchange_public_key() {
        generate_key_rings();
    }

    #[test]
    fn test_crc64_verification() {
        let (server, client) = generate_key_rings();

        let pkt_slices = vec![
            Bytes::from("a"),
            Bytes::from("bcde"),
            Bytes::new(),
            Bytes::from("fghij"),
        ];

        let verification_type = PacketVerifyType::CRC64;
        let signature = server.sign(verification_type, pkt_slices.iter().map(|b| b.as_bytes()));
        dbg!(hex::encode_upper(&signature));

        let whole_packet = pkt_slices
            .into_iter()
            .fold(BytesMut::new(), |mut buffer, slice| {
                buffer.extend(slice);
                buffer
            })
            .freeze();

        let whole_packet = PacketVerificationData::CRC64 {
            pkt: whole_packet.as_bytes(),
            crc64: signature.as_bytes(),
        };

        client.verify(whole_packet).unwrap();
    }

    #[test]
    fn test_ed25519_verification() {
        let (server, client) = generate_key_rings();

        let pkt_slices = vec![
            Bytes::from("a"),
            Bytes::from("bcde"),
            Bytes::new(),
            Bytes::from("fghij"),
        ];

        let verification_type = PacketVerifyType::Ed25519;
        let signature = client.sign(verification_type, pkt_slices.iter().map(|b| b.as_bytes()));
        let derived_public_key = client.derive_public_key().unwrap();

        dbg!(hex::encode_upper(&signature));
        dbg!(hex::encode_upper(&derived_public_key));

        let whole_packet = pkt_slices
            .into_iter()
            .fold(BytesMut::new(), |mut buffer, slice| {
                buffer.extend(slice);
                buffer
            })
            .freeze();

        let whole_packet = PacketVerificationData::Ed25519 {
            pkt: whole_packet.as_bytes(),
            pub_key: &derived_public_key,
            signature: &signature,
        };

        server.verify(whole_packet.clone()).unwrap();

        KeyRing::new(vec![], None)
            .verify(whole_packet)
            .expect_err("Should fail when no pubkey");
    }
}
