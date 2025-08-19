use blake3;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::{Arc, atomic::AtomicU32};
use std::time::Duration;
use usync::util::file::ChunkIndex;

use tokio::sync::Semaphore;

use usync::constants::TRANSMISSION_INFO_LENGTH;
use usync::engine::{Bus, BusAddress, BusMessage, decoding, receiving, sending};
use usync::protocol::coding::raptorq_code::{RaptorqReceiver, RaptorqSender};
use usync::protocol::mock_init;
use usync::transmission::mock::MockSocket;
use usync::util::{
    file::{CHUNK_INDEX, write_at},
    generate_random,
    log::init as init_log,
};

const CONCURRENCY: usize = 10;
const CHUNKS: u32 = 20;
const CHUNK_SIZE: usize = 1048576;

#[tokio::main]
async fn main() {
    debug_assert!(
        false,
        "Run in release mode instead for raptorq is too slow in debug mode."
    );
    use tempfile::NamedTempFile;
    let mut file = NamedTempFile::new().unwrap();

    init_log("localtest.log".into());

    let data = generate_random(CHUNK_SIZE);
    let expected_hash = blake3::hash(&data);
    dbg!(&expected_hash);

    let path = OsString::from(file.path().as_os_str());

    write_at(&path, 0, &data).unwrap();

    let mut check_read = vec![];
    let length = file.read_to_end(&mut check_read).unwrap();
    assert_eq!(length, CHUNK_SIZE);
    assert_eq!(&expected_hash, &blake3::hash(&check_read));

    CHUNK_INDEX
        .set(ChunkIndex {
            files: HashMap::from([(0, path.clone())]),
            chunks: HashMap::from_iter(
                (0..CHUNKS).map(|chunk_id| (chunk_id, (0usize, 0u64, CHUNK_SIZE))),
            ),
        })
        .map_err(|_| "Failed to init OnceLock")
        .unwrap();

    let addr1: SocketAddr = "127.0.0.1:10000".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:10001".parse().unwrap();
    let (sock1, sock2) = MockSocket::pair(addr1, addr2);

    mock_init();

    let bus: Arc<Bus<BusAddress, BusMessage<TRANSMISSION_INFO_LENGTH>>> = Arc::new(Bus::default());
    let sender = sending::SendingSocket::new(sock1, bus.clone().register(BusAddress::SenderSocket));
    tokio::spawn(sender.run::<RaptorqSender>());
    let receiver =
        receiving::ReceivingSocket::new(sock2, bus.clone().register(BusAddress::ReceiverSocket));
    tokio::spawn(receiver.run(addr1));

    let sem = Arc::new(Semaphore::new(CONCURRENCY));
    let finish = Arc::new(AtomicU32::new(CHUNKS));

    for chunk_id in 0..CHUNKS {
        let sem = sem.clone();
        let bus = bus.clone();
        let finish = finish.clone();

        let waiting = |finish: Arc<AtomicU32>| async move {
            let permit = sem.acquire().await.unwrap();
            let handler =
                decoding::spawn::<RaptorqReceiver, TRANSMISSION_INFO_LENGTH>(chunk_id, bus.clone());
            let result = handler.await.unwrap().unwrap();
            drop(permit);
            println!(
                " {} Finished, length {}, hash {:?}",
                chunk_id,
                result.len(),
                blake3::hash(&result)
            );
            finish.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        };
        tokio::spawn(waiting(finish));
    }

    while finish.load(std::sync::atomic::Ordering::Relaxed) > 0 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        bus.debug();
    }
}
