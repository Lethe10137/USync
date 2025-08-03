use crate::protocol::constants::TRANSMISSION_INFO_LENGTH;

pub trait FrameSender {
    fn init(chunk_data: &[u8], next_id: u32) -> Self;
    fn next_frame(&mut self) -> (u32, Vec<u8>);
    fn get_trasmission_info(&self) -> [u8; TRANSMISSION_INFO_LENGTH];
}

pub trait FrameReceiver: Sized {
    fn try_init(frame: &[u8]) -> Option<Self>;
    fn update(frame: &[u8]) -> Option<Vec<u8>>;
}

mod raptorq_code;
