use crate::protocol::wire::encoding::RawParts;
use crate::protocol::wire::frames::{FrameType, ParsedFrameVariant};
use crate::protocol::wire::packets::{PacketType, ParsedPacketVariant};
use crate::protocol::wire::verify::PacketVerifyType;

use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

use bytes::Bytes;

use zerocopy::byteorder::{BigEndian, U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

pub mod encoding;
pub mod frames;
pub mod packets;
pub mod verify;

static ID_COUNTER: AtomicU32 = AtomicU32::new(0);
fn new_packet_id() -> u32 {
    ID_COUNTER.fetch_add(1, Relaxed)
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout, Debug)]
pub struct CommonPacketHeader {
    version: u8,
    packet_type: u8,
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
    frame_type: u8,
    frame_length: U16<BigEndian>,
}

pub trait SpecificFrameHeader: RawParts {
    fn get_frame_type(&self) -> FrameType;
}

pub trait Frame: Sized {
    type Header: SpecificFrameHeader;
    fn header(&self) -> &Self::Header;
    fn body_len(&self) -> usize {
        0
    }
    fn take_body(self) -> Option<Bytes> {
        None
    }
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedFrameVariant<'a>>;
}

pub struct BuiltFrame {
    header: Bytes,
    body: Option<Bytes>,
}

pub trait Packet: Sized {
    type Header: SpecificPacketHeader;

    fn packet_type(&self) -> PacketType;
    fn get_header(&self) -> &Self::Header;
    fn get_body(self) -> impl Iterator<Item = BuiltFrame>;
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedPacketVariant<'a>>;
    fn verification_type() -> PacketVerifyType;
}
