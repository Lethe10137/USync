use bytes::{BufMut, BytesMut};
use flume::{Receiver, Sender, unbounded};
use std::sync::OnceLock;
use std::{
    fs::OpenOptions,
    io::{self, Write},
};

use zerocopy::IntoBytes;

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
pub fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

fn current_timestamp_ns() -> u64 {
    (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos()
        & 0xFFFF_FFFF_FFFF_FFFF) as u64
}

struct PacketLog {
    time_ns: u64,
    pkt_number: u32,
    magic: u32,
}

static LOGGER: OnceLock<Sender<PacketLog>> = OnceLock::new();

pub fn packet_log(pkt_number: u32, magic: u32) {
    if let Some(logger) = LOGGER.get() {
        let log = PacketLog {
            time_ns: current_timestamp_ns(),
            pkt_number,
            magic,
        };
        let _ = logger.send(log);
    }
}

fn log_writer(rx: Receiver<PacketLog>, log_file: PathBuf) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)?;

    for _ in 0..4 {
        file.write_all(0x12345678u32.as_bytes())?;
    }

    while let Ok(log) = rx.recv() {
        let mut writer = BytesMut::with_capacity(16).writer();
        writer.write_all(log.time_ns.as_bytes())?;
        writer.write_all(log.pkt_number.as_bytes())?;
        writer.write_all(log.magic.as_bytes())?;
        file.write_all(writer.get_ref().as_bytes())?;
    }
    Ok(())
}

pub fn init(name: PathBuf) {
    let (logger_tx, logger_rx) = unbounded();
    std::thread::spawn(move || log_writer(logger_rx, name));
    let _ = LOGGER.set(logger_tx);
}
