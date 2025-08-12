use bytes::{Buf, Bytes, BytesMut};

use crate::constants::VERSION;
use crate::protocol::key_ring::KEY_RING;

use crate::protocol::wire::{
    BuiltFrame, CommonFrameHeader, CommonPacketHeader, Frame, FrameType, Packet, PacketType,
    ParsedFrameVariant, ParsedPacketVariant, SpecificFrameHeader, verify::PacketVerificationError,
};

use zerocopy::{FromBytes, Immutable, IntoBytes, TryFromBytes, Unaligned};

pub trait RawParts: IntoBytes + FromBytes + Unaligned + Sized + Immutable {
    fn raw_len() -> usize {
        std::mem::size_of::<Self>()
    }
}
impl<T> RawParts for T where T: IntoBytes + FromBytes + Unaligned + Immutable {}

pub(crate) trait PacketExt: Packet {
    fn build(self) -> Vec<Bytes> {
        let header_length = (
            CommonPacketHeader::raw_len(),
            <Self as Packet>::Header::raw_len(),
        );
        let packet_type = Self::PACKET_TYPE;
        let dummy_common_header = Bytes::new();
        let mut body_length: usize = 0;
        let mut header = BytesMut::with_capacity(header_length.1);
        header.extend_from_slice(self.get_header().as_bytes());
        debug_assert!(header.len() == header_length.1);
        let mut result = vec![dummy_common_header, header.freeze()];

        for frame in self.get_body() {
            body_length += frame.header.len();
            result.push(frame.header);
            if let Some(frame_body) = frame.body {
                body_length += frame_body.len();
                result.push(frame_body);
            }
        }

        let packet_header = CommonPacketHeader {
            version: VERSION,
            packet_type: packet_type.into(),
            header_length: ((header_length.0 + header_length.1) as u16).into(),
            body_length: (body_length as u16).into(),
            packet_id: super::new_packet_id().into(),
        };

        let mut common_header = BytesMut::with_capacity(header_length.0);
        common_header.extend_from_slice(packet_header.as_bytes());
        debug_assert!(common_header.len() == header_length.0);
        *result.get_mut(0).unwrap() = common_header.freeze();

        // CRC64 or ED25519
        let signature = KEY_RING.get().unwrap().sign(
            Self::PACKET_VERIFICATION_TYPE,
            result.iter().map(|pkt| pkt.as_bytes()),
        );
        result.push(signature);
        result
    }
}
impl<T: Packet> PacketExt for T {}

pub(super) trait FrameExt: Frame {
    fn total_header_len(&self) -> usize {
        CommonFrameHeader::raw_len() + <Self as Frame>::Header::raw_len()
    }

    fn build(self) -> BuiltFrame {
        let header_length = self.total_header_len();

        let frame_length: u16 = (header_length + self.body_len()).try_into().unwrap();
        let common_header = CommonFrameHeader {
            frame_type: self.header().get_frame_type().into(),
            frame_length: frame_length.into(),
        };
        let mut header = BytesMut::with_capacity(header_length);
        header.extend_from_slice(common_header.as_bytes());
        header.extend_from_slice(self.header().as_bytes());
        debug_assert_eq!(header_length, header.len());

        BuiltFrame {
            header: header.freeze(),
            body: self.take_body(),
        }
    }
}

impl<T: Frame> FrameExt for T {}

#[derive(Debug)]
pub struct ParsedPacket<const INFO_LENGTH: usize> {
    pub pkt: Bytes,
    pub specific_packet_header: ParsedPacketVariant,
    pub frames: Vec<ParsedFrameVariant<INFO_LENGTH>>,
}

impl<const INFO_LENGTH: usize> ParsedPacket<INFO_LENGTH> {
    pub fn get_common_packet_header(&self) -> &CommonPacketHeader {
        let (header, _remain) =
            CommonPacketHeader::try_ref_from_prefix(self.pkt.as_bytes()).unwrap();
        header
    }
}

#[derive(Debug)]
pub enum ParseError {
    UnsupportedVerion(u8),
    UnsupportedPacketType(u8),
    UnsupportedFrameType(u8),
    InconsistentFields,
    PacketTooShort,
    BodyTooshort,
    Verification(PacketVerificationError),
    FailedToParsePacketHeader,
    FailedToParseFrame,
}

fn parse_frame<const INFO_LENGTH: usize>(
    mut remained_body: Bytes,
) -> Result<Vec<ParsedFrameVariant<INFO_LENGTH>>, ParseError> {
    let mut frames = vec![];

    while !remained_body.is_empty() {
        let (common_frame_header, _) =
            CommonFrameHeader::try_ref_from_prefix(remained_body.as_bytes())
                .map_err(|_| ParseError::BodyTooshort)?;
        let frame_type = common_frame_header.frame_type;
        let frame_length = u16::from(common_frame_header.frame_length) as usize;

        let current_frame = if frame_length < CommonFrameHeader::raw_len() {
            eprintln!("Insane frame length");
            return Err(ParseError::BodyTooshort);
        } else {
            &remained_body[CommonFrameHeader::raw_len()..frame_length]
        };

        let current_frame = FrameType::try_from(frame_type)
            .map_err(|_| ParseError::UnsupportedFrameType(frame_type))?
            .try_parse(remained_body.slice_ref(current_frame))
            .ok_or(ParseError::UnsupportedFrameType(frame_type))?;

        frames.push(current_frame);
        remained_body.advance(frame_length);
    }

    Ok(frames)
}

pub fn parse_packet<const INFO_LENGTH: usize>(
    packet: Bytes,
) -> Result<ParsedPacket<INFO_LENGTH>, ParseError> {
    let (common_packet_header, _) = CommonPacketHeader::try_ref_from_prefix(packet.as_bytes())
        .map_err(|_| ParseError::PacketTooShort)?;
    let header_length = u16::from(common_packet_header.header_length) as usize;
    let body_length = u16::from(common_packet_header.body_length) as usize;
    if common_packet_header.version != VERSION {
        eprintln!("Unsupported version {}", common_packet_header.version);
        return Err(ParseError::UnsupportedVerion(common_packet_header.version));
    }

    let verification_field = if header_length + body_length > packet.len() {
        eprintln!("Packet too short");
        return Err(ParseError::PacketTooShort);
    } else {
        &packet[header_length + body_length..]
    };

    // Todo: LOG here!
    let _packet_id = u32::from(common_packet_header.packet_id);

    let specific_packet_header = if header_length < CommonPacketHeader::raw_len() {
        eprintln!("Insane packet header length");
        return Err(ParseError::InconsistentFields);
    } else {
        &packet[CommonPacketHeader::raw_len()..header_length]
    };

    let packet_variant = PacketType::try_from(common_packet_header.packet_type)
        .map_err(|_| ParseError::UnsupportedPacketType(common_packet_header.packet_type))?
        .try_parse::<INFO_LENGTH>(packet.slice_ref(specific_packet_header))
        .ok_or(ParseError::FailedToParsePacketHeader)?;

    KEY_RING
        .get()
        .unwrap()
        .verify(
            packet_variant.build_verification_data(
                &packet[..header_length + body_length],
                verification_field,
            ),
        )
        .map_err(ParseError::Verification)?;

    let remained_body = packet.slice_ref(&packet[header_length..header_length + body_length]);

    let frames = parse_frame(remained_body)?;
    Ok(ParsedPacket {
        pkt: packet,
        specific_packet_header: packet_variant,
        frames,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::constants::*;
    use crate::protocol::key_ring::mock_init;
    use crate::protocol::wire::frames::{GetChunkFrameHeader, ParsedFrameVariant};
    use crate::protocol::wire::packets::current_timestamp_ms;
    use bytes::BytesMut;

    fn build_into_bytes(vec: Vec<Bytes>) -> Bytes {
        let mut total_packet = BytesMut::new();
        for item in vec.iter() {
            total_packet.extend_from_slice(&item);
        }
        total_packet.freeze()
    }

    #[test]
    fn build_parse_data_packet() {
        mock_init();

        use crate::protocol::wire::packets::DataPacket;
        let mock_data: Vec<u8> = vec![88; DEFAULT_FRAME_LEN];
        let data_packet = DataPacket::new(
            19260817,
            85213,
            [7u8; TRANSMISSION_INFO_LENGTH],
            mock_data.clone(),
        );
        let built = data_packet.build();

        let total_packet = build_into_bytes(built);

        assert_eq!(
            DEFAULT_FRAME_LEN % 16,
            0,
            "Default frame len should be 16-aligned."
        );

        assert!(total_packet.len() <= MTU);

        let parsed_packet =
            parse_packet::<TRANSMISSION_INFO_LENGTH>(Bytes::from(total_packet)).unwrap();

        if let ParsedFrameVariant::Data(data_frame) = &parsed_packet.frames[0] {
            assert_eq!(19260817, data_frame.chunk_id);
            assert_eq!(85213, data_frame.frame_offset);

            assert_eq!(mock_data, data_frame.data);
        } else {
            unreachable!()
        }
    }

    #[test]
    fn build_parse_ticket_packet() {
        mock_init();
        use crate::protocol::wire::packets::TicketPacket;

        let start_time = current_timestamp_ms();

        let packet = TicketPacket::new()
            .set_rate_limit(80000)
            .set_get_chunk(8, 75, 400) // Should be shadowed!
            .set_get_chunk(17, 2334, 800)
            .set_get_chunk(8, 234, 600)
            .build();

        let total_packet = build_into_bytes(packet);
        assert!(total_packet.len() <= MTU);

        let parsed_packet =
            parse_packet::<TRANSMISSION_INFO_LENGTH>(Bytes::from(total_packet)).unwrap();

        let current_time = current_timestamp_ms();

        if let ParsedPacketVariant::TicketPacket {
            pub_key,
            timestamp_ms,
        } = parsed_packet.specific_packet_header
        {
            assert_eq!(
                *pub_key,
                KEY_RING.get().unwrap().derive_public_key().unwrap()
            );
            assert!(start_time <= timestamp_ms && timestamp_ms <= current_time);
        } else {
            unreachable!();
        }

        let mut expected = HashMap::new();
        expected.insert(8, (234, 600));
        expected.insert(17, (2334, 800));
        let mut rate_limit = None;

        for frame in parsed_packet.frames {
            match frame {
                ParsedFrameVariant::RateLimit(header) => {
                    assert!(
                        rate_limit
                            .replace(u32::from(header.desired_max_kbps))
                            .is_none()
                    )
                }
                ParsedFrameVariant::GetChunk(GetChunkFrameHeader {
                    chunk_id,
                    next_receive_offset,
                    receive_window_frames,
                }) => {
                    let expected_entry = expected.remove(&u32::from(chunk_id)).unwrap();
                    assert_eq!(expected_entry.0, u32::from(next_receive_offset));
                    assert_eq!(expected_entry.1, u32::from(receive_window_frames));
                }
                _ => unreachable!(),
            }
        }

        assert_eq!(expected.len(), 0);
        assert_eq!(rate_limit, Some(80000));
    }
}
