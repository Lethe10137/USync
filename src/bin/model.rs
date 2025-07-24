use rand::random_bool;

use rand::Rng;
use raptorq::{Decoder, Encoder, EncodingPacket};

use blake3;
use flume::{Receiver, Sender, bounded};
use std::time::{Duration, Instant};
use std::{collections::BTreeMap, thread};

const FILESIZE: usize = 32 * 1024 * 1024;
const PACKET_SIZE: u16 = 1450;
const LOSS_RATE: f64 = 0.2;

//nix supports iovec!

type Packet = Vec<u8>;

fn sender(encoder: Encoder, sending: Sender<Packet>, closing: Receiver<()>) {
    let block_encoders = encoder.get_block_encoders();
    let encoder_cnt = block_encoders.len();
    println!("{encoder_cnt} encoders");

    let source_packets = block_encoders
        .iter()
        .flat_map(|encoder| encoder.source_packets().into_iter());

    let repair_packets = block_encoders
        .iter()
        .cycle()
        .enumerate()
        .map(|(i, encoder)| (i / encoder_cnt, encoder))
        .flat_map(|(round, encoder)| {
            encoder
                .repair_packets((16 * round) as u32, round as u32)
                .into_iter()
        });

    let mut packets_iter = source_packets
        .chain(repair_packets)
        .map(|packet| packet.serialize());

    let mut packets_cnt = 0;

    while let Some(packet) = packets_iter.next()
        && closing.try_recv() == Err(flume::TryRecvError::Empty)
    {
        match sending.send_deadline(packet, Instant::now() + Duration::from_secs(1)) {
            Ok(_) => packets_cnt += 1,
            Err(flume::SendTimeoutError::Disconnected(_)) => {
                break;
            }
            Err(flume::SendTimeoutError::Timeout(_)) => (),
        }
    }

    println!(
        "Sender Finished, total {} pkts, {} Bytes",
        packets_cnt,
        packets_cnt * PACKET_SIZE as usize
    );
}

fn reciever(mut decoder: Decoder, network_recieve: Receiver<Packet>) {
    // Perform the decoding
    let mut result = None;
    let mut packets_cnt = 0;
    while let Ok(packet) = network_recieve.recv() {
        packets_cnt += 1;
        result = decoder.decode(EncodingPacket::deserialize(packet.as_slice()));
        if result.is_some() {
            break;
        }
    }
    println!(
        "Receives Finished, total {} pkts, {} Bytes",
        packets_cnt,
        packets_cnt * PACKET_SIZE as usize
    );

    if let Some(result) = result {
        println!("Result Hash is {}", blake3::hash(result.as_slice()));
    } else {
        println!("Failed to decrpt");
    }
}
fn main() {
    // Generate some random data to send
    let mut data: Vec<u8> = vec![0; FILESIZE];
    for byte in data.iter_mut() {
        *byte = rand::rng().random();
    }

    println!("Original Hash is {}", blake3::hash(data.as_slice()));

    let (sending_packet, receiving_packet) = bounded::<Packet>(128);
    let (network_in, network_out) = bounded::<Packet>(128);
    let (sending_stop, receiving_stop) = bounded::<()>(1);

    // Create the Encoder, with an MTU of 1400 (common for Ethernet)
    let encoder = Encoder::with_defaults(data.as_slice(), PACKET_SIZE);

    // The Decoder MUST be constructed with the configuration of the Encoder.
    // The ObjectTransmissionInformation configuration should be transmitted over a reliable
    // channel
    let decoder = Decoder::new(encoder.get_config());

    thread::scope(|s| {
        s.spawn(|| sender(encoder, sending_packet, receiving_stop));

        s.spawn(|| {
            let mut lossy_packets_channel = receiving_packet
                .iter()
                .filter(|_| random_bool(1.0 - LOSS_RATE));

            // Reording
            let mut network: BTreeMap<usize, Packet> = BTreeMap::new();

            'outer: loop {
                while network.len() < 32 {
                    if let Some(packet) = lossy_packets_channel.next() {
                        network.insert(rand::random_range(usize::MIN..usize::MAX), packet);
                    } else {
                        break 'outer;
                    }
                    if network_in.receiver_count() == 0 {
                        break 'outer;
                    };
                }

                while network.len() > 16 {
                    let (_, packet) = network.pop_first().unwrap();
                    if network_in.send(packet).is_err() {
                        break 'outer; // Receiver Finishes
                    }
                    if network_in.receiver_count() == 0 {
                        break 'outer;
                    };
                }
            }
            println!("Stop Send to Sender!");
            sending_stop.send(()).unwrap();
        });
        s.spawn(|| reciever(decoder, network_out));
    });
}
