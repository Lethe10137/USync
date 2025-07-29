use super::PacketType;

use crate::protocol::constants::{CHUNK_SIZE, MTU};
use crate::protocol::packets::frames::DataFrame;
use crate::protocol::packets::{FrameExt, Packet, SpecificPacketHeader};
use bytes::Bytes;
use zerocopy::byteorder::{BigEndian, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

pub const DATA_PACKET: PacketType = 0b1000_0001;
pub const TICKET_PACKET: PacketType = 0b0100_0001;

#[derive(Debug)]
pub enum ParsedPacketVariant<'a> {
    DataPacket(),
    TicketPacket { pubkey: &'a [u8] },
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout)]
pub struct DataPacketHeader {}

impl SpecificPacketHeader for DataPacketHeader {
    fn get_packet_type(&self) -> PacketType {
        DATA_PACKET
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
        DATA_PACKET
    }
    fn get_header(&self) -> &Self::Header {
        &self.header
    }
    fn get_body(self) -> impl Iterator<Item = super::BuiltFrame> {
        let built = self.data.build();
        std::iter::once(built)
    }
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedPacketVariant<'a>> {
        (data.len() == 0).then_some(ParsedPacketVariant::DataPacket())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::*;
    use crate::protocol::packets::{PacketExt, frames::ParsedFrameVariant, parse_packet};
    use bytes::BytesMut;
    use hex;
    #[test]
    fn build_parse_data_packet() {
        let mock_data: Vec<u8> = vec![88; DEFAULT_FRAME_LEN];
        let data_packet = DataPacket::new(19260817, 85213, mock_data.clone());
        let built = data_packet.build();

        let mut total_packet = BytesMut::new();
        for item in built.iter() {
            println!("{} {}", item.len(), hex::encode_upper(item));
            total_packet.extend_from_slice(&item);
        }

        assert_eq!(
            DEFAULT_FRAME_LEN % 16,
            0,
            "Default frame len should be 16-aligned."
        );

        let total_packet = total_packet.freeze();
        assert!(total_packet.len() <= MTU);

        let parsed_packet = parse_packet(&total_packet).unwrap();

        if let ParsedFrameVariant::DataFrame(data_frame) = &parsed_packet.frames[0] {
            assert_eq!(19260817, data_frame.chunk_id);
            assert_eq!(85213, data_frame.frame_offset);
            assert_eq!(CHUNK_SIZE as u32, data_frame.chunk_size);
            assert_eq!(mock_data, data_frame.data);
        } else {
            unreachable!()
        }
    }
}
