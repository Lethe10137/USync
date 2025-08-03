use bytes::Bytes;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use zerocopy::byteorder::{BigEndian, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::protocol::constants::TRANSMISSION_INFO_LENGTH;

use super::{Frame, SpecificFrameHeader};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
pub enum FrameType {
    Data = 0x01,
    GetChunk = 0x02,
    RateLimit = 0x03,
}

impl FrameType {
    pub(super) fn try_parse<'a>(&self, data: &'a [u8]) -> Option<ParsedFrameVariant<'a>> {
        match &self {
            FrameType::Data => DataFrame::try_parse(data),
            FrameType::GetChunk => GetChunkFrame::try_parse(data),
            FrameType::RateLimit => RateLimitFrame::try_parse(data),
        }
    }
}

#[derive(Debug)]
pub enum ParsedFrameVariant<'a> {
    Data(ParsedDataFrame<'a>),
    GetChunk(GetChunkFrameHeader),
    RateLimit(RateLimitFrameHeader),
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout)]
pub struct DataFrameHeader {
    pub chunk_id: U32<BigEndian>,
    pub frame_offset: U32<BigEndian>,
    pub transmission_info: [u8; TRANSMISSION_INFO_LENGTH],
}

impl SpecificFrameHeader for DataFrameHeader {
    fn get_frame_type(&self) -> FrameType {
        FrameType::Data
    }
}

pub struct DataFrame {
    header: DataFrameHeader,
    data: Bytes,
}
#[derive(Debug)]
pub struct ParsedDataFrame<'a> {
    pub chunk_id: u32,
    pub frame_offset: u32,
    pub transmission_info: [u8; TRANSMISSION_INFO_LENGTH],
    pub data: &'a [u8],
}

impl DataFrame {
    pub fn new(
        chunk_id: u32,
        frame_offset: u32,
        transmission_info: [u8; TRANSMISSION_INFO_LENGTH],
        data: Bytes,
    ) -> Self {
        Self {
            header: DataFrameHeader {
                chunk_id: chunk_id.into(),
                frame_offset: frame_offset.into(),
                transmission_info,
            },
            data,
        }
    }
}

impl Frame for DataFrame {
    type Header = DataFrameHeader;

    fn header(&self) -> &Self::Header {
        &self.header
    }
    fn body_len(&self) -> usize {
        self.data.len()
    }
    fn take_body(self) -> Option<Bytes> {
        Some(self.data)
    }
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedFrameVariant<'a>> {
        let (header, data) = DataFrameHeader::read_from_prefix(data).ok()?;
        ParsedFrameVariant::Data(ParsedDataFrame {
            chunk_id: header.chunk_id.into(),
            frame_offset: header.frame_offset.into(),
            transmission_info: header.transmission_info,
            data,
        })
        .into()
    }
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout, Debug)]
pub struct GetChunkFrameHeader {
    pub chunk_id: U32<BigEndian>,
    pub max_received_offset: U32<BigEndian>,
    pub receive_window_frames: U32<BigEndian>, // 0 means send no more!
}

impl SpecificFrameHeader for GetChunkFrameHeader {
    fn get_frame_type(&self) -> FrameType {
        FrameType::GetChunk
    }
}

pub type GetChunkFrame = GetChunkFrameHeader;
pub type PrasedGetChunkFrame = GetChunkFrameHeader;
impl Frame for GetChunkFrame {
    type Header = GetChunkFrameHeader;
    fn header(&self) -> &Self::Header {
        self
    }
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedFrameVariant<'a>> {
        let (header, remain) = GetChunkFrameHeader::read_from_prefix(data).ok()?;
        remain
            .is_empty()
            .then_some(ParsedFrameVariant::GetChunk(header))
    }
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout, Debug)]
pub struct RateLimitFrameHeader {
    pub desired_max_kbps: U32<BigEndian>,
}

impl SpecificFrameHeader for RateLimitFrameHeader {
    fn get_frame_type(&self) -> FrameType {
        FrameType::RateLimit
    }
}

pub type RateLimitFrame = RateLimitFrameHeader;
pub type ParsedRateLimitFrame = RateLimitFrame;
impl Frame for RateLimitFrame {
    type Header = RateLimitFrameHeader;
    fn header(&self) -> &Self::Header {
        self
    }
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedFrameVariant<'a>> {
        let (header, remain) = RateLimitFrameHeader::read_from_prefix(data).ok()?;

        remain
            .is_empty()
            .then_some(ParsedFrameVariant::RateLimit(header))
    }
}
