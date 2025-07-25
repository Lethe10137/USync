use binrw::{BinRead, BinWrite};

use crate::protocol::constants::*;
use crc::{CRC_64_ECMA_182, Crc};
use std::io::{Cursor, IoSlice};
use std::sync::atomic::{AtomicU32, Ordering};

static ID_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(BinRead, BinWrite, Debug, Clone)]
#[brw(big)] // Big Endian
pub struct DataPacketHeader {
    version: u8,
    packet_type: u8,
    data_len: u16,
    chunk_size: u32,
    chunk_id: u32,
    packet_id: u32,
}

impl DataPacketHeader {
    pub fn new(chunk_id: u32, chunk_size: u32) -> Self {
        DataPacketHeader {
            version: VERSION,
            packet_type: DATA_PACKET,
            data_len: 0, // Was overwritten when constructing `DataPacket`, so no need to be filled here.
            chunk_size: chunk_size,
            chunk_id: chunk_id,
            packet_id: ID_COUNTER.fetch_add(1, Ordering::Relaxed),
        }
    }
}

pub struct DataPacket {
    header: Vec<u8>,
    data: Vec<u8>,
    crc64: [u8; 8], // Big endian
}

impl DataPacket {
    pub fn new(mut header: DataPacketHeader, data: Vec<u8>) -> Self {
        header.data_len = data.len() as u16;
        let mut header_buf = Vec::new();
        header_buf.reserve_exact(std::mem::size_of_val(&header));
        let mut header_buf = Cursor::new(header_buf);
        header.write(&mut header_buf).unwrap();
        let header_buf = header_buf.into_inner();

        let crc64 = Crc::<u64>::new(&CRC_64_ECMA_182);
        let mut digest = crc64.digest();
        digest.update(&header_buf);
        digest.update(&data);
        DataPacket {
            header: header_buf,
            data,
            crc64: digest.finalize().to_be_bytes(),
        }
    }

    pub fn as_io_slice(&self) -> [IoSlice; 3] {
        [
            IoSlice::new(&self.header),
            IoSlice::new(&self.data),
            IoSlice::new(&self.crc64),
        ]
    }
}

pub struct ParsedDataPacket<'a> {
    pub header: DataPacketHeader,
    pub data: &'a [u8],
}

impl<'a> ParsedDataPacket<'a> {
    pub fn parse(input: &'a [u8]) -> Result<Self, String> {
        let header_size = std::mem::size_of::<DataPacketHeader>();
        if input.len() < header_size + 8 {
            return Err("Packet too short".to_string());
        }

        let mut cursor = Cursor::new(&input[..header_size]);
        let header: DataPacketHeader = BinRead::read(&mut cursor).map_err(|e| e.to_string())?;

        let total_len = header_size + header.data_len as usize + 8;
        if input.len() < total_len {
            return Err("Packet data too short".to_string());
        }

        let data = &input[header_size..header_size + header.data_len as usize];
        let crc_from_packet = &input[header_size + header.data_len as usize..total_len];

        // 重新计算 CRC64
        let crc64 = Crc::<u64>::new(&CRC_64_ECMA_182);
        let mut digest = crc64.digest();
        digest.update(&input[..header_size]);
        digest.update(data);
        let expected_crc = digest.finalize().to_be_bytes();

        if expected_crc != crc_from_packet {
            return Err("CRC mismatch".to_string());
        }

        Ok(ParsedDataPacket { header, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_packet_to_bytes(packet: &DataPacket) -> Vec<u8> {
        let mut bytes =
            Vec::with_capacity(packet.header.len() + packet.data.len() + packet.crc64.len());
        bytes.extend_from_slice(&packet.header);
        bytes.extend_from_slice(&packet.data);
        bytes.extend_from_slice(&packet.crc64);
        bytes
    }

    #[test]
    fn test_packet_roundtrip() {
        let header = DataPacketHeader::new(42, 1024);
        let data = b"hello world!".to_vec();
        let packet = DataPacket::new(header.clone(), data.clone());

        let bytes = data_packet_to_bytes(&packet);
        let parsed = ParsedDataPacket::parse(&bytes).unwrap();

        assert_eq!(parsed.header.version, header.version);
        assert_eq!(parsed.header.packet_type, header.packet_type);
        assert_eq!(parsed.header.chunk_id, header.chunk_id);
        assert_eq!(parsed.header.chunk_size, header.chunk_size);
        assert_eq!(parsed.header.packet_id, header.packet_id);
        assert_eq!(parsed.data, &data[..]);
    }

    #[test]
    fn test_packet_crc_mismatch() {
        let header = DataPacketHeader::new(1, 1);
        let data = b"bad data".to_vec();
        let packet = DataPacket::new(header, data);

        let mut bytes = data_packet_to_bytes(&packet);
        let len = bytes.len();
        // 破坏 CRC
        bytes[len - 1] ^= 0xFF;

        assert!(ParsedDataPacket::parse(&bytes).is_err());
    }
}
