use crate::constants::TRANSMISSION_INFO_LENGTH;
use bytes::Bytes;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt;
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
    pub(super) fn try_parse<const INFO_LENGTH: usize>(
        &self,
        data: Bytes,
    ) -> Option<ParsedFrameVariant<INFO_LENGTH>> {
        match &self {
            FrameType::Data => DataFrame::<TRANSMISSION_INFO_LENGTH>::try_parse(data),
            FrameType::GetChunk => GetChunkFrame::try_parse(data),
            FrameType::RateLimit => RateLimitFrame::try_parse(data),
        }
    }
}

#[derive(Debug)]
pub enum ParsedFrameVariant<const INFO_LENGTH: usize> {
    Data(ParsedDataFrame<INFO_LENGTH>),
    GetChunk(GetChunkFrameHeader),
    RateLimit(RateLimitFrameHeader),
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout, Debug)]
pub struct DataFrameHeader<const INFO_LENGTH: usize> {
    pub chunk_id: U32<BigEndian>,
    pub frame_offset: U32<BigEndian>,
    pub transmission_info: [u8; INFO_LENGTH],
}

impl<const INFO_LENGTH: usize> SpecificFrameHeader for DataFrameHeader<INFO_LENGTH> {
    fn get_frame_type(&self) -> FrameType {
        FrameType::Data
    }
}

pub struct DataFrame<const INFO_LENGTH: usize> {
    header: DataFrameHeader<INFO_LENGTH>,
    data: Bytes,
}

pub struct ParsedDataFrame<const INFO_LENGTH: usize> {
    pub chunk_id: u32,
    pub frame_offset: u32,
    pub transmission_info: [u8; INFO_LENGTH],
    pub data: Bytes,
}

fn preview_bytes(bytes: &Bytes) -> String {
    let len = bytes.len();
    let preview_len = 16.min(len);
    let preview: Vec<String> = bytes
        .iter()
        .take(preview_len)
        .map(|b| format!("{b:02x}"))
        .collect();
    format!(
        "[{} bytes: {}{}]",
        len,
        preview.join(" "),
        if len > preview_len { " ..." } else { "" }
    )
}

impl<const INFO_LENGTH: usize> fmt::Debug for DataFrame<INFO_LENGTH> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataFrame")
            .field("header", &self.header)
            .field("data", &preview_bytes(&self.data))
            .finish()
    }
}

impl<const INFO_LENGTH: usize> fmt::Debug for ParsedDataFrame<INFO_LENGTH> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedDataFrame")
            .field("chunk_id", &self.chunk_id)
            .field("frame_offset", &self.frame_offset)
            .field("transmission_info", &self.transmission_info)
            .field("data", &preview_bytes(&self.data))
            .finish()
    }
}

impl<const INFO_LENGTH: usize> DataFrame<INFO_LENGTH> {
    pub fn new(
        chunk_id: u32,
        frame_offset: u32,
        transmission_info: [u8; INFO_LENGTH],
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

impl<const INFO_LEN: usize> Frame for DataFrame<INFO_LEN> {
    type Header = DataFrameHeader<INFO_LEN>;

    fn header(&self) -> &Self::Header {
        &self.header
    }
    fn body_len(&self) -> usize {
        self.data.len()
    }
    fn take_body(self) -> Option<Bytes> {
        Some(self.data)
    }
    fn try_parse<const INFO_LENGTH: usize>(
        frame: Bytes,
    ) -> Option<ParsedFrameVariant<INFO_LENGTH>> {
        let (header, data) = DataFrameHeader::read_from_prefix(frame.as_bytes()).ok()?;
        ParsedFrameVariant::Data(ParsedDataFrame {
            chunk_id: header.chunk_id.into(),
            frame_offset: header.frame_offset.into(),
            transmission_info: header.transmission_info,
            data: frame.slice_ref(data),
        })
        .into()
    }
}

#[repr(C)]
#[derive(IntoBytes, FromBytes, Unaligned, Immutable, KnownLayout, Debug)]
pub struct GetChunkFrameHeader {
    pub chunk_id: U32<BigEndian>,
    pub next_receive_offset: U32<BigEndian>,
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
    fn try_parse<const INFO_LENGTH: usize>(data: Bytes) -> Option<ParsedFrameVariant<INFO_LENGTH>> {
        let (header, remain) = GetChunkFrameHeader::read_from_prefix(data.as_bytes()).ok()?;
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
    fn try_parse<const INFO_LENGTH: usize>(data: Bytes) -> Option<ParsedFrameVariant<INFO_LENGTH>> {
        let (header, remain) = RateLimitFrameHeader::read_from_prefix(data.as_bytes()).ok()?;

        remain
            .is_empty()
            .then_some(ParsedFrameVariant::RateLimit(header))
    }
}
