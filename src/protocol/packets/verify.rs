use bytes::Bytes;

pub fn sign(mode: PacketVerifyType, data: &[Bytes]) -> Bytes {
    Bytes::from("abcdefgh")
}
pub fn check_ok(
    mode: PacketVerifyType,
    data: &Bytes,
    signature: &Bytes,
    identification: Option<Bytes>,
) -> bool {
    true
}

pub enum PacketVerifyType {
    None,
    CRC64,
    Ed25519,
}
