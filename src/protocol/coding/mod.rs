pub trait FrameSender<const TRANSMISSION_INFO_LENGTH: usize> {
    fn init(chunk_data: impl AsRef<[u8]>, next_id: u32) -> Self;
    fn next_frame(&mut self) -> (u32, Vec<u8>);
    fn get_trasmission_info(&self) -> [u8; TRANSMISSION_INFO_LENGTH];
}

pub trait FrameReceiver<const TRANSMISSION_INFO_LENGTH: usize>: Sized {
    fn try_init(frame: &[u8; TRANSMISSION_INFO_LENGTH]) -> Option<Self>;
    fn update(&mut self, frame_id: u32, frame: &[u8]) -> Option<Vec<u8>>;
    fn expected_frame_id(&self) -> u32;
}

pub mod raptorq_code;
