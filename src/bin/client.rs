use anyhow::anyhow;
use clap::Parser;
use directories::UserDirs;
use humansize::{BINARY, format_size};
use owo_colors::OwoColorize;
use std::str::FromStr;
use std::sync::{Arc, atomic::AtomicUsize};
use std::{fs, net::SocketAddr, path::PathBuf};
use tokio::sync::Semaphore;
use tokio::time::Duration;
use usync::constants::TRANSMISSION_INFO_LENGTH;
use usync::engine::{Bus, BusAddress, BusMessage, decoding, receiving};
use usync::protocol::{coding::raptorq_code::RaptorqReceiver, init};
use usync::transmission::real::RealUdpSocket;
use usync::util::{
    file::{check_file_exist_create, mmap_segment, write_at},
    plan::{FileChunk, FileConfig},
};
use zerocopy::IntoBytes;

#[derive(Parser, Debug)]
#[command(author, version, about = "Client for receiving file", long_about = None)]
struct Args {
    /// The path to the plan file (TOML format).
    #[arg(short, long, value_name = "PLAN_FILE")]
    plan_file: PathBuf,

    /// Socket Addr of Server
    #[arg(short, long, value_name = "SERVER")]
    server: SocketAddr,

    /// Private Key
    #[arg(short, long, value_name = "PRI_KEY")]
    private_key: String,

    /// The path to the downloading file (optional, in your download folder as default).
    #[arg(short, long, value_name = "DOWNLOADING_FILE")]
    downloading_file: Option<PathBuf>,
}

fn check_chunks<'b>(path: &PathBuf, config: &'b FileConfig) -> Vec<&'b FileChunk> {
    let mut result = vec![];
    for chunk in config.chunks.iter() {
        result.push(chunk);

        print!(
            ">>> Checking chunk {:04}: ...",
            chunk.chunk_id.bright_blue()
        );

        let hash = match mmap_segment(path, chunk.offset, chunk.length) {
            Ok(chunk_data) => hex::encode(blake3::hash(chunk_data.as_bytes()).as_bytes()),
            Err(err) => {
                println!("\x1b[3D {}: {err:#}", "Failed to read".yellow());
                continue;
            }
        };

        if hash.as_str() != chunk.hash {
            println!(
                "\x1b[3D {}. Expected {}, actual {}",
                "Hash check failed".red(),
                chunk.hash.yellow(),
                hash.yellow()
            );
            continue;
        }
        println!("\x1b[3D {}", "OK".green());
        result.pop();
    }
    result
}

fn check_file<'a>(
    downloading_file: &PathBuf,
    config: &'a FileConfig,
) -> anyhow::Result<Vec<&'a FileChunk>> {
    println!(
        "{} chunks in total for file {}.",
        config.chunks.len(),
        downloading_file.display()
    );

    let need_to_download = check_chunks(downloading_file, config);
    let download_size: usize = need_to_download.iter().map(|chunk| chunk.length).sum();

    let print_config = BINARY.decimal_places(3).decimal_zeroes(3);
    println!(
        "Need to download {} / {} chunks which sized {} / {}.",
        need_to_download.len().yellow(),
        config.chunks.len().blue(),
        format_size(download_size, print_config).yellow(),
        format_size(config.total_length, print_config).blue(),
    );
    Ok(need_to_download)
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    debug_assert!(
        false,
        "Run in release mode instead for raptorq is too slow in debug mode."
    );

    let args = Args::parse();

    // Init key ring.
    init(vec![], Some(args.private_key));

    let toml_str = fs::read_to_string(&args.plan_file)?;
    let config: FileConfig = toml::from_str(&toml_str)?;

    let downloading_file = match args.downloading_file {
        Some(path) => path,
        None => {
            let user_dir = UserDirs::new();
            let downloads_dir = user_dir.as_ref().and_then(UserDirs::document_dir)
            .ok_or(anyhow!(
                "Failed to determine downloading path. Please explictly designate one with --downloading-file."
            ))?;

            downloads_dir.join(&config.file_name)
        }
    };

    println!("Downloading file: {}", downloading_file.display());

    if check_file_exist_create(&downloading_file)? {
        println!("{} already exists.", downloading_file.display(),);
    } else {
        println!(
            "Created {} successfully as an empty file.",
            downloading_file.display()
        )
    }

    let bus: Arc<Bus<BusAddress, BusMessage<TRANSMISSION_INFO_LENGTH>>> = Arc::new(Bus::default());
    let socket = RealUdpSocket::bind(SocketAddr::from_str("0.0.0.0:0").unwrap())
        .await
        .unwrap();
    let receiver =
        receiving::ReceivingSocket::new(socket, bus.clone().register(BusAddress::ReceiverSocket));
    tokio::spawn(receiver.run(args.server));

    let need_to_download = check_file(&downloading_file, &config)?;

    let semaphore = Arc::new(Semaphore::new(8));
    let finish = Arc::new(AtomicUsize::new(need_to_download.len()));

    for to_download in need_to_download {
        let to_download = to_download.clone();
        let semaphore = semaphore.clone();
        let bus = bus.clone();
        let finish = finish.clone();
        let downloading_file = downloading_file.clone();

        let chunk_id = to_download.chunk_id as u32;

        let waiting = |finish: Arc<AtomicUsize>| async move {
            let permit = semaphore.acquire().await.unwrap();
            let result =
                decoding::spawn::<RaptorqReceiver, TRANSMISSION_INFO_LENGTH>(chunk_id, bus.clone())
                    .await;

            drop(permit);
            let Ok(Some(result)) = result else {
                eprintln!(
                    "Downloaded chunk {} currupted.",
                    to_download.chunk_id.on_red(),
                );
                finish.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                return;
            };

            let hash = hex::encode(blake3::hash(&result).as_bytes());
            if hash == to_download.hash && result.len() == to_download.length {
                write_at(downloading_file, to_download.offset, &result).ok();
                eprintln!(
                    "Succeed in download chunk {}, at [{},{})",
                    to_download.chunk_id.green(),
                    to_download.offset.magenta(),
                    (to_download.offset + to_download.length as u64).magenta()
                )
            } else {
                eprintln!(
                    "Downloaded chunk {} currupted.",
                    to_download.chunk_id.on_red(),
                )
            }

            finish.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        };
        tokio::spawn(waiting(finish));
    }

    while finish.load(std::sync::atomic::Ordering::Relaxed) > 0 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        bus.debug();
    }

    Ok(())
}
