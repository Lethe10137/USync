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
    StopChunk = 0x03,
    RateLimit = 0x04,
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
    GetChunk(&'a [u8]),
    StopChunk(&'a [u8]),
    RateLimit(&'a [u8]),
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
        if let Ok((header, data)) = DataFrameHeader::read_from_prefix(data) {
            Some(ParsedFrameVariant::Data(ParsedDataFrame {
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
