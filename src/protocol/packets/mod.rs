use crate::protocol::constants::VERSION;
use crate::protocol::packets::frames::{
    DATA_FRAME, DataFrame, GET_CHUNK_FRAME, ParsedFrameVariant, RATE_LIMIT_FRAME, STOP_CHUNK_FRAME,
};
use crate::protocol::packets::packets::{
    DATA_PACKET, DataPacket, ParsedPacketVariant, TICKET_PACKET,
};
use crate::protocol::packets::verify::{PacketVerifyType, sign};
use bytes::{Buf, Bytes, BytesMut};
use std::sync::atomic::{AtomicU32, Ordering};
use zerocopy::byteorder::{BigEndian, U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

pub mod frames;
pub mod packets;
pub mod verify;

pub type PacketType = u8;
pub type FrameType = u8;

static ID_COUNTER: AtomicU32 = AtomicU32::new(0);
pub fn new_packet_id() -> u32 {
    ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn choose_verify_mode(packet_type: PacketType) -> PacketVerifyType {
    match packet_type {
        DATA_PACKET => PacketVerifyType::CRC64,
        TICKET_PACKET => PacketVerifyType::Ed25519,
        _ => PacketVerifyType::None,
    }
}

pub trait RawParts: IntoBytes + FromBytes + Unaligned + Sized + Immutable {
    fn raw_len() -> usize {
        std::mem::size_of::<Self>()
    }
}
impl<T> RawParts for T where T: IntoBytes + FromBytes + Unaligned + Immutable {}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout, Debug)]
pub struct CommonPacketHeader {
    version: u8,
    packet_type: PacketType,
    header_length: U16<BigEndian>,
    body_length: U16<BigEndian>,
    packet_id: U32<BigEndian>,
}

pub trait SpecificPacketHeader: RawParts {
    fn get_packet_type(&self) -> PacketType;
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout, Debug)]
struct CommonFrameHeader {
    frame_type: FrameType,
    frame_length: U16<BigEndian>,
}

pub trait SpecificFrameHeader: RawParts {
    fn get_frame_type(&self) -> FrameType;
}

pub trait Frame: Sized {
    type Header: SpecificFrameHeader;
    fn header<'a>(&'a self) -> &'a Self::Header;
    fn body_len(&self) -> usize;
    fn take_body(self) -> Option<Bytes>;
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedFrameVariant<'a>>;
}

pub struct BuiltFrame {
    header: Bytes,
    body: Option<Bytes>,
}

pub trait FrameExt: Frame {
    fn total_header_len(&self) -> usize {
        CommonFrameHeader::raw_len() + <Self as Frame>::Header::raw_len()
    }

    fn build(self) -> BuiltFrame {
        let header_length = self.total_header_len();

        let frame_length: u16 = (header_length + self.body_len()).try_into().unwrap();
        let common_header = CommonFrameHeader {
            frame_type: self.header().get_frame_type(),
            frame_length: frame_length.into(),
        };
        let mut header = BytesMut::with_capacity(header_length);
        header.extend_from_slice(&common_header.as_bytes());
        header.extend_from_slice(&self.header().as_bytes());
        debug_assert_eq!(header_length, header.len());

        BuiltFrame {
            header: header.freeze(),
            body: self.take_body(),
        }
    }
}

impl<T: Frame> FrameExt for T {}

pub trait Packet: Sized {
    type Header: SpecificPacketHeader;

    fn packet_type(&self) -> PacketType;
    fn get_header(&self) -> &Self::Header;
    fn get_body(self) -> impl Iterator<Item = BuiltFrame>;
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedPacketVariant<'a>>;
}

trait PacketExt: Packet {
    fn build(self) -> Vec<Bytes> {
        let header_length = (
            CommonPacketHeader::raw_len(),
            <Self as Packet>::Header::raw_len(),
        );

        let packet_type = self.packet_type();

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
            packet_type: packet_type,
            header_length: ((header_length.0 + header_length.1) as u16).into(),
            body_length: (body_length as u16).into(),
            packet_id: new_packet_id().into(),
        };

        let mut common_header = BytesMut::with_capacity(header_length.0);
        common_header.extend_from_slice(packet_header.as_bytes());
        debug_assert!(common_header.len() == header_length.0);
        *result.get_mut(0).unwrap() = common_header.freeze();

        let signature = sign(choose_verify_mode(packet_type), &result);
        result.push(signature);

        result
    }
}
impl<T: Packet> PacketExt for T {}

#[derive(Debug)]
pub struct ParsedPacket<'a> {
    pub common_packet_header: &'a CommonPacketHeader,
    pub specific_packet_header: ParsedPacketVariant<'a>,
    pub frames: Vec<ParsedFrameVariant<'a>>,
    pub verification_field: &'a [u8],
}

#[derive(Debug)]
pub enum ParseError {
    UnsupportedVerion(u8),
    UnsupportedPacketType(u8),
    UnsupportedFrameType(u8),
    InconsistentFields,
    PacketTooShort,
    BodyTooshort,
    Verification,
    FailedToParsePacketHeader,
    FailedToParseFrame,
}

pub fn parse_packet<'a>(packet: &'a [u8]) -> Result<ParsedPacket<'a>, ParseError> {
    let (common_packet_header, _) =
        CommonPacketHeader::try_ref_from_prefix(packet).map_err(|_| ParseError::PacketTooShort)?;
    let packet_type = common_packet_header.packet_type;
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

    dbg!(verification_field);
    // Todo: LOG here!
    let _packet_id = u32::from(common_packet_header.packet_id);

    let specific_packet_header = if header_length < CommonPacketHeader::raw_len() {
        eprintln!("Insane packet header length");
        return Err(ParseError::InconsistentFields);
    } else {
        &packet[CommonPacketHeader::raw_len()..header_length]
    };

    let specific_packet_header = match common_packet_header.packet_type {
        DATA_PACKET => DataPacket::try_parse(specific_packet_header)
            .ok_or(ParseError::FailedToParsePacketHeader)?,
        TICKET_PACKET => todo!(),
        _ => {
            eprintln!(
                "Unsupported Packet type {}",
                common_packet_header.packet_type
            );
            return Err(ParseError::UnsupportedPacketType(
                common_packet_header.packet_type,
            ));
        }
    };

    let mut remained_body = &packet[header_length..header_length + body_length];
    let mut frames = vec![];

    while remained_body.len() > 0 {
        let (common_frame_header, _) = CommonFrameHeader::try_ref_from_prefix(remained_body)
            .map_err(|_| ParseError::BodyTooshort)?;
        let frame_type = common_frame_header.frame_type;
        let frame_length = u16::from(common_frame_header.frame_length) as usize;

        let current_frame = if frame_length < CommonFrameHeader::raw_len() {
            eprintln!("Insane frame length");
            return Err(ParseError::BodyTooshort);
        } else {
            &remained_body[CommonFrameHeader::raw_len()..frame_length]
        };

        let current_frame = match frame_type {
            DATA_FRAME => {
                DataFrame::try_parse(current_frame).ok_or(ParseError::FailedToParseFrame)?
            }
            GET_CHUNK_FRAME => todo!(),
            STOP_CHUNK_FRAME => todo!(),
            RATE_LIMIT_FRAME => todo!(),
            _ => {
                eprintln!("Unsupported Frame type {}", frame_type);
                return Err(ParseError::UnsupportedFrameType(frame_type));
            }
        };
        frames.push(current_frame);
        remained_body.advance(frame_length);
    }

    Ok(ParsedPacket {
        common_packet_header,
        specific_packet_header,
        frames,
        verification_field,
    })
}
