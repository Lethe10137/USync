use clap::Parser;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::BufRead;

use std::sync::Arc;
use std::{fs, net::SocketAddr, path::PathBuf};
use tokio::time::Duration;
use usync::constants::TRANSMISSION_INFO_LENGTH;
use usync::engine::{Bus, BusAddress, BusMessage, sending};
use usync::protocol::{coding::raptorq_code::RaptorqSender, init};
use usync::transmission::real::RealUdpSocket;
use usync::util::{
    file::{CHUNK_INDEX, ChunkIndex, check_file_exist},
    plan::FileConfig,
};

#[derive(Parser, Debug)]
#[command(author, version, about = "Server for sending file", long_about = None)]
struct Args {
    /// The path to the plan file (TOML format).
    #[arg(short, long, value_name = "PLAN_FILE")]
    plan_file: PathBuf,

    /// Listening addr
    #[arg(short, long, value_name = "LISTEN")]
    listening: SocketAddr,

    /// The path to authorized public key, one per line.
    #[arg(short, long, value_name = "PUB_KEY")]
    public_key: PathBuf,

    /// The path to the folder that contains the  file to be downloaded.
    #[arg(short, long, value_name = "DOWNLOAD_FOLDER")]
    folder: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    debug_assert!(
        false,
        "Run in release mode instead for raptorq is too slow in debug mode."
    );

    let args = Args::parse();

    let public_key_file = File::open(args.public_key).unwrap();
    let lines = std::io::BufReader::new(public_key_file)
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    init(lines, None);

    let toml_str = fs::read_to_string(&args.plan_file)?;
    let config: FileConfig = toml::from_str(&toml_str)?;

    let downloading_file = args.folder.join(config.file_name);
    println!("Downloading file: {}", downloading_file.display());

    check_file_exist(&downloading_file)?;
    println!("{} already exists.", downloading_file.display());

    CHUNK_INDEX
        .set(ChunkIndex {
            files: HashMap::from([(0usize, OsString::from(downloading_file))]),
            chunks: HashMap::from_iter(
                config
                    .chunks
                    .iter()
                    .map(|chunk| (chunk.chunk_id as u32, (0usize, chunk.offset, chunk.length))),
            ),
        })
        .map_err(|_| "Failed to init OnceLock")
        .unwrap();

    let bus: Arc<Bus<BusAddress, BusMessage<TRANSMISSION_INFO_LENGTH>>> = Arc::new(Bus::default());
    let socket = RealUdpSocket::bind(args.listening).await.unwrap();
    let sender =
        sending::SendingSocket::new(socket, bus.clone().register(BusAddress::SenderSocket));
    tokio::spawn(sender.run::<RaptorqSender>());
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        bus.debug();
    }
}
