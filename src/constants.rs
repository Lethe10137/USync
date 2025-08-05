pub const VERSION: u8 = 1;

pub const MTU: usize = 1490;
pub const DEFAULT_PAGE_SIZE: usize = 4096;
pub const DEFAULT_PAGE_CHUNKS: usize = 8192;
pub const CHUNK_SIZE: usize = DEFAULT_PAGE_CHUNKS * DEFAULT_PAGE_SIZE;

pub const DEFAULT_FRAME_LEN: usize = 1440;
pub const PUB_KEY_LENGTH: usize = 32;
pub const PRI_KEY_LENGTH: usize = 32;
pub const SIGNATURE_LENGTH: usize = 32;

pub const TRANSMISSION_INFO_LENGTH: usize = 12;
