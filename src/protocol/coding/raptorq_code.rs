use crate::protocol::constants::DEFAULT_FRAME_LEN;

use super::FrameSender;
use raptorq::{Encoder, ObjectTransmissionInformation};

use std::collections::VecDeque;

pub struct RaptorqSender {
    encoder: Encoder,
    config: ObjectTransmissionInformation,
    cache: VecDeque<(u32, Vec<u8>)>,
    next_fetch_id: usize,
}

impl FrameSender for RaptorqSender {
    fn init(chunk_data: &[u8], next_id: u32) -> Self {
        let config = ObjectTransmissionInformation::with_defaults(
            chunk_data.len() as u64,
            DEFAULT_FRAME_LEN as u16,
        );
        let encoder = Encoder::new(chunk_data, config);
        let next_fetch_id = next_id as usize / encoder.get_block_encoders().len();
        RaptorqSender {
            encoder,
            config,
            cache: VecDeque::new(),
            next_fetch_id,
        }
    }

    fn next_frame(&mut self) -> (u32, Vec<u8>) {
        const BURST: usize = 32;
        if self.cache.is_empty() {
            let encoder_cnt = self.encoder.get_block_encoders().len();

            let mut new_data = Vec::new();

            for encoder in self.encoder.get_block_encoders() {
                let data = encoder.get_range(self.next_fetch_id, BURST);
                new_data.push(data);
            }

            for _ in 0..BURST {
                for (i, frame) in new_data.iter_mut().enumerate() {
                    self.cache.push_back((
                        (i + self.next_fetch_id * encoder_cnt) as u32,
                        frame.next().unwrap().serialize(),
                    ));
                }
                self.next_fetch_id += 1;
            }
        }
        self.cache.pop_front().unwrap()
    }

    fn get_trasmission_info(&self) -> [u8; crate::protocol::constants::TRANSMISSION_INFO_LENGTH] {
        self.encoder.get_config().serialize()
    }
}

#[cfg(test)]
mod test {

    const FILESIZE: usize = 1048576;
    use rand::Rng;

    use crate::protocol::{
        coding::{FrameSender, raptorq_code::RaptorqSender},
        constants::MTU,
    };

    fn generate_random() -> Vec<u8> {
        let mut data: Vec<u8> = vec![0; FILESIZE];
        for byte in data.iter_mut() {
            *byte = rand::rng().random();
        }
        data
    }

    #[test]
    fn get_gen_frames() {
        let data = generate_random();
        let mut generator = RaptorqSender::init(&data, 64);
        for (i, (j, data)) in std::iter::from_fn(|| generator.next_frame().into())
            .enumerate()
            .take(200)
        {
            assert_eq!(i + 64, j as usize);
            assert!(data.len() <= MTU);
        }
    }
}
