use super::encoding::FrameExt;
use super::frames::DataFrame;
use super::verify::PacketVerificationData;
use super::{Packet, SpecificPacketHeader};
use crate::protocol::constants::CHUNK_SIZE;

use bytes::Bytes;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[repr(u8)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive, Unaligned, Immutable,
)]

pub enum PacketType {
    Data = 0b1000_0001,
    Ticket = 0b0100_0001,
}

impl PacketType {
    pub(super) fn try_parse<'a>(&self, data: &'a [u8]) -> Option<ParsedPacketVariant<'a>> {
        match &self {
            PacketType::Data => DataPacket::try_parse(data),
            PacketType::Ticket => todo!(),
        }
    }
}

#[derive(Debug)]
pub enum ParsedPacketVariant<'a> {
    DataPacket(),
    TicketPacket { pub_key: &'a [u8] },
}

impl<'a> ParsedPacketVariant<'a> {
    pub fn build_verification_data(
        &'a self,
        pkt: &'a [u8],
        verification_field: &'a [u8],
    ) -> PacketVerificationData<'a> {
        match self {
            ParsedPacketVariant::DataPacket() => PacketVerificationData::CRC64 {
                pkt,
                crc64: verification_field,
            },
            ParsedPacketVariant::TicketPacket { pub_key } => PacketVerificationData::Ed25519 {
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

pub struct DataPacket {
    header: DataPacketHeader,
    data: DataFrame,
}

impl DataPacket {
    pub fn new(chunk_id: u32, offset: u32, data: Vec<u8>) -> Self {
        Self {
            header: DataPacketHeader {},
            data: DataFrame::new(chunk_id, CHUNK_SIZE as u32, offset, Bytes::from(data)),
        }
    }
}

impl Packet for DataPacket {
    type Header = DataPacketHeader;
    fn packet_type(&self) -> PacketType {
        PacketType::Data
    }
    fn get_header(&self) -> &Self::Header {
        &self.header
    }
    fn get_body(self) -> impl Iterator<Item = super::BuiltFrame> {
        let built = self.data.build();
        std::iter::once(built)
    }
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedPacketVariant<'a>> {
        (data.is_empty()).then_some(ParsedPacketVariant::DataPacket())
    }
    fn verification_type() -> super::verify::PacketVerifyType {
        super::verify::PacketVerifyType::CRC64
    }
}
