use bytes::Bytes;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use zerocopy::byteorder::{BigEndian, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

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
            _ => todo!(),
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
    pub chunk_size: U32<BigEndian>,
    pub frame_offset: U32<BigEndian>,
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
    pub chunk_size: u32,
    pub frame_offset: u32,
    pub data: &'a [u8],
}

impl DataFrame {
    pub fn new(chunk_id: u32, chunk_size: u32, frame_offset: u32, data: Bytes) -> Self {
        Self {
            header: DataFrameHeader {
                chunk_id: chunk_id.into(),
                chunk_size: chunk_size.into(),
                frame_offset: frame_offset.into(),
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
            chunk_size: header.chunk_size.into(),
            frame_offset: header.frame_offset.into(),
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
        &self
    }
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedFrameVariant<'a>> {
        let (header, remain) = GetChunkFrameHeader::read_from_prefix(data).ok()?;
        if remain.len() != 0 {
            return None;
        }
        ParsedFrameVariant::GetChunk(header).into()
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
        &self
    }
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedFrameVariant<'a>> {
        let (header, remain) = RateLimitFrameHeader::read_from_prefix(data).ok()?;
        if remain.len() != 0 {
            return None;
        }
        ParsedFrameVariant::RateLimit(header).into()
    }
}
