use super::FrameType;
use bytes::Bytes;
use zerocopy::byteorder::{BigEndian, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

pub const DATA_FRAME: FrameType = 0x01;
pub const GET_CHUNK_FRAME: FrameType = 0x02;
pub const STOP_CHUNK_FRAME: FrameType = 0x03;
pub const RATE_LIMIT_FRAME: FrameType = 0x04;

use crate::protocol::packets::{Frame, SpecificFrameHeader};

#[derive(Debug)]
pub enum ParsedFrameVariant<'a> {
    DataFrame(ParsedDataFrame<'a>),
    GetChunkFrame(&'a [u8]),
    StopChunkFrame(&'a [u8]),
    RateLimitFrame(&'a [u8]),
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
        DATA_FRAME
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

    fn header<'a>(&'a self) -> &'a Self::Header {
        &self.header
    }
    fn body_len(&self) -> usize {
        self.data.len()
    }
    fn take_body(self) -> Option<Bytes> {
        Some(self.data)
    }
    fn try_parse<'a>(data: &'a [u8]) -> Option<ParsedFrameVariant<'a>> {
        if let Ok((header, data)) = DataFrameHeader::read_from_prefix(data) {
            Some(ParsedFrameVariant::DataFrame(ParsedDataFrame {
                chunk_id: header.chunk_id.into(),
                chunk_size: header.chunk_size.into(),
                frame_offset: header.frame_offset.into(),
                data,
            }))
        } else {
            None
        }
    }
}
