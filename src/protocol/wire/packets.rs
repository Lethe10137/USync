use std::collections::HashMap;

use super::encoding::FrameExt;
use super::frames::DataFrame;
use super::verify::PacketVerificationData;
use super::{Packet, SpecificPacketHeader};
use crate::constants::PUB_KEY_LENGTH;
use crate::protocol::key_ring::KEY_RING;
use crate::protocol::wire::frames::{GetChunkFrame, RateLimitFrame};
use crate::protocol::wire::verify::PacketVerifyType;

use bytes::{Buf, Bytes};
use ed25519_dalek::PUBLIC_KEY_LENGTH;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use zerocopy::{BigEndian, FromBytes, Immutable, IntoBytes, KnownLayout, U64, Unaligned};

use std::time::{SystemTime, UNIX_EPOCH};

pub fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

#[repr(u8)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive, Unaligned, Immutable,
)]

pub enum PacketType {
    Data = 0b1000_0001,
    Ticket = 0b0100_0001,
}

impl PacketType {
    pub(super) fn try_parse<const INFO_LENGTH: usize>(
        &self,
        data: Bytes,
    ) -> Option<ParsedPacketVariant> {
        match &self {
            PacketType::Data => DataPacket::<INFO_LENGTH>::try_parse(data),
            PacketType::Ticket => TicketPacket::try_parse(data),
        }
    }
}

#[derive(Debug)]
pub enum ParsedPacketVariant {
    DataPacket(),
    TicketPacket { pub_key: Bytes, timestamp_ms: u64 },
}

impl ParsedPacketVariant {
    pub fn build_verification_data<'a>(
        &'a self,
        pkt: &'a [u8],
        verification_field: &'a [u8],
    ) -> PacketVerificationData<'a> {
        match self {
            ParsedPacketVariant::DataPacket() => PacketVerificationData::CRC64 {
                pkt,
                crc64: verification_field,
            },
            ParsedPacketVariant::TicketPacket { pub_key, .. } => PacketVerificationData::Ed25519 {
                pkt,
                pub_key,
                signature: verification_field,
            },
        }
    }
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout)]
pub struct DataPacketHeader {}

impl SpecificPacketHeader for DataPacketHeader {
    fn get_packet_type(&self) -> PacketType {
        PacketType::Data
    }
}

pub struct DataPacket<const INFO_LENGTH: usize> {
    header: DataPacketHeader,
    data: DataFrame<INFO_LENGTH>, // DataFrame<12> for raptorq
}

impl<const INFO_LENGTH: usize> From<DataFrame<INFO_LENGTH>> for DataPacket<INFO_LENGTH> {
    fn from(data: DataFrame<INFO_LENGTH>) -> Self {
        Self {
            header: DataPacketHeader {},
            data,
        }
    }
}

impl<const INFO_LENGTH: usize> DataPacket<INFO_LENGTH> {
    pub fn new(
        chunk_id: u32,
        offset: u32,
        transmission_info: [u8; INFO_LENGTH],
        data: Vec<u8>,
    ) -> Self {
        Self {
            header: DataPacketHeader {},
            data: DataFrame::new(chunk_id, offset, transmission_info, Bytes::from(data)),
        }
    }
}

impl<const INFO_LENGTH: usize> Packet for DataPacket<INFO_LENGTH> {
    type Header = DataPacketHeader;
    const PACKET_TYPE: PacketType = PacketType::Data;
    const PACKET_VERIFICATION_TYPE: PacketVerifyType = PacketVerifyType::CRC64;

    fn get_header(&self) -> &Self::Header {
        &self.header
    }
    fn get_body(self) -> impl Iterator<Item = super::BuiltFrame> {
        let built = self.data.build();
        std::iter::once(built)
    }
    fn try_parse(data: Bytes) -> Option<ParsedPacketVariant> {
        (data.is_empty()).then_some(ParsedPacketVariant::DataPacket())
    }
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout)]
pub struct TicketPacketHeader {
    pub pubkey: [u8; PUBLIC_KEY_LENGTH],
    pub timestamp_ms: U64<BigEndian>,
}

impl SpecificPacketHeader for TicketPacketHeader {
    fn get_packet_type(&self) -> PacketType {
        PacketType::Ticket
    }
}

pub struct TicketPacket {
    header: TicketPacketHeader,
    rate_limit: Option<RateLimitFrame>,
    get_chunk: HashMap<u32, GetChunkFrame>,
}

impl Default for TicketPacket {
    fn default() -> Self {
        Self::new()
    }
}

impl TicketPacket {
    pub fn new() -> Self {
        let pubkey = KEY_RING
            .get()
            .and_then(|key_ring| key_ring.derive_public_key())
            .expect("Failed to derive public key");
        Self {
            header: TicketPacketHeader {
                pubkey,
                timestamp_ms: current_timestamp_ms().into(),
            },
            rate_limit: None,
            get_chunk: HashMap::new(),
        }
    }
    pub fn set_rate_limit(mut self, rate_kpbs: u32) -> Self {
        self.rate_limit = Some(RateLimitFrame {
            desired_max_kbps: rate_kpbs.into(),
        });
        self
    }

    pub fn set_get_chunk(
        mut self,
        chunk_id: u32,
        next_received_offset: u32,
        receive_window: u32,
    ) -> Self {
        self.get_chunk.insert(
            chunk_id,
            GetChunkFrame {
                chunk_id: chunk_id.into(),
                next_receive_offset: next_received_offset.into(),
                receive_window_frames: receive_window.into(),
            },
        );
        self
    }
}

impl Packet for TicketPacket {
    type Header = TicketPacketHeader;
    const PACKET_TYPE: PacketType = PacketType::Ticket;
    const PACKET_VERIFICATION_TYPE: PacketVerifyType = PacketVerifyType::Ed25519;

    fn get_header(&self) -> &Self::Header {
        &self.header
    }
    fn get_body(self) -> impl Iterator<Item = super::BuiltFrame> {
        let rate_limit = self
            .rate_limit
            .map(|rate_limit| rate_limit.build())
            .into_iter();

        let get_packets = self.get_chunk.into_values().map(|frame| frame.build());

        rate_limit.chain(get_packets)
    }
    fn try_parse(data: Bytes) -> Option<ParsedPacketVariant> {
        let (pub_key, mut remain): (&[u8], &[u8]) =
            data.as_bytes().split_at_checked(PUB_KEY_LENGTH)?;
        let pub_key: &[u8; PUB_KEY_LENGTH] = pub_key.try_into().ok()?;
        let timestamp_ms = remain.try_get_u64().ok()?;

        remain
            .is_empty()
            .then_some(ParsedPacketVariant::TicketPacket {
                pub_key: data.slice_ref(pub_key),
                timestamp_ms,
            })
    }
}
