use rand::Rng;
use raptorq::{Encoder, ObjectTransmissionInformation};

const FILESIZE: usize = 16 * 1024 * 1024 + 7;
const PACKET_SIZE: u16 = 1500;

pub fn main() {
    debug_assert!(
        false,
        "Please run in release mod, since raptorq will be far more faster."
    );

    let mut data: Vec<u8> = vec![0; FILESIZE];
    for byte in data.iter_mut() {
        *byte = rand::rng().random();
    }

    let config = ObjectTransmissionInformation::with_defaults(FILESIZE as u64, PACKET_SIZE);

    let encoder = Encoder::new(&data, config);
    let encoders = encoder.get_block_encoders();
    assert_eq!(encoders.len(), 1);
    let encoder_1 = &encoders[0];

    let mut hasher1 = blake3::Hasher::new();
    let mut hasher2 = blake3::Hasher::new();

    let source_packets = encoder_1.source_packets();
    assert_eq!(11215, source_packets.len());
    let repair_packets = encoder_1.repair_packets(0, 23000 - 11215);

    for packet in source_packets {
        hasher1.update(packet.serialize().as_slice());
    }

    for packet in repair_packets {
        hasher1.update(packet.serialize().as_slice());
    }

    for i in 0..1000 {
        for (j, packet) in encoder_1.get_range(i * 23, 23).enumerate() {
            assert_eq!(
                packet.payload_id().encoding_symbol_id() as usize,
                j + i * 23
            );
            hasher2.update(packet.serialize().as_slice());
        }
    }

    let hash1 = hasher1.finalize();
    let hash2 = hasher2.finalize();

    dbg!(hash1, hash2);

    assert_eq!(hash1, hash2);
}
