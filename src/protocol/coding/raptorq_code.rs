use crate::protocol::{coding::FrameReceiver, constants::DEFAULT_FRAME_LEN};

use super::FrameSender;
use crate::protocol::constants::TRANSMISSION_INFO_LENGTH as RAPTORQ_TRANSMISSION_INFO_LENGTH;
use raptorq::{Decoder, Encoder, EncodingPacket, ObjectTransmissionInformation};

use std::collections::VecDeque;

pub struct RaptorqSender {
    encoder: Encoder,
    config: ObjectTransmissionInformation,
    cache: VecDeque<(u32, Vec<u8>)>,
    next_fetch_id: usize,
}

impl FrameSender<RAPTORQ_TRANSMISSION_INFO_LENGTH> for RaptorqSender {
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

    fn get_trasmission_info(&self) -> [u8; RAPTORQ_TRANSMISSION_INFO_LENGTH] {
        self.encoder.get_config().serialize()
    }
}

pub struct RaptorqReceiver {
    decoder: Decoder,
    expected_frame_id: u32,
}

impl FrameReceiver<RAPTORQ_TRANSMISSION_INFO_LENGTH> for RaptorqReceiver {
    fn try_init(frame: &[u8; RAPTORQ_TRANSMISSION_INFO_LENGTH]) -> Option<Self> {
        let config = ObjectTransmissionInformation::deserialize(frame);
        let decoder = Decoder::new(config);
        Self {
            decoder,
            expected_frame_id: 0,
        }
        .into()
    }
    fn update(&mut self, frame_id: u32, frame: &[u8]) -> Option<Vec<u8>> {
        self.expected_frame_id = self.expected_frame_id.max(frame_id + 1);
        self.decoder.decode(EncodingPacket::deserialize(frame))
    }
    fn expected_frame_id(&self) -> u32 {
        self.expected_frame_id
    }
}

#[cfg(test)]
mod test {

    const CHUNK_SIZE: usize = 1048576;
    use rand::Rng;

    use crate::protocol::{
        coding::{
            FrameReceiver, FrameSender,
            raptorq_code::{RaptorqReceiver, RaptorqSender},
        },
        constants::MTU,
    };

    fn generate_random(size: usize) -> Vec<u8> {
        let mut data: Vec<u8> = vec![0; size];
        for byte in data.iter_mut() {
            *byte = rand::rng().random();
        }
        data
    }

    #[test]
    fn get_gen_frames() {
        let data = generate_random(CHUNK_SIZE);
        let mut generator = RaptorqSender::init(&data, 64);
        for (i, (j, data)) in std::iter::from_fn(|| generator.next_frame().into())
            .enumerate()
            .take(200)
        {
            assert_eq!(i + 64, j as usize);
            assert!(data.len() <= MTU);
        }
    }

    #[test]
    fn decoding() {
        let data = generate_random(CHUNK_SIZE);
        let mut encoder = RaptorqSender::init(&data, 0);

        let config = encoder.get_trasmission_info();
        let mut decoder = RaptorqReceiver::try_init(&config).unwrap();

        for i in 0..600 {
            let (frame_id, frame) = encoder.next_frame();
            if i % 5 != 0 {
                decoder.update(frame_id, &frame);
            }
        }

        // Mock a restart

        let restart_id = decoder.expected_frame_id();
        let mut encoder = RaptorqSender::init(&data, restart_id);

        let restored_data = loop {
            let (frame_id, frame) = encoder.next_frame();
            assert!(frame_id < 1000, "Take too long!");
            if let Some(restored_data) = decoder.update(frame_id, &frame) {
                break restored_data;
            }
        };

        assert_eq!(data, restored_data);
    }
}
