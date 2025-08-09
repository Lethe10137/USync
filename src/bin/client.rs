use anyhow::anyhow;
use clap::Parser;
use directories::UserDirs;
use humansize::{BINARY, format_size};
use owo_colors::OwoColorize;
use std::{fs, path::PathBuf};
use zerocopy::IntoBytes;

use usync::util::{
    file::{check_file_exist, mmap_segment},
    plan::{FileChunk, FileConfig},
};

#[derive(Parser, Debug)]
#[command(author, version, about = "Client for receiving file", long_about = None)]
struct Args {
    /// The path to the plan file (TOML format).
    #[arg(short, long, value_name = "PLAN_FILE")]
    plan_file: PathBuf,

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

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

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

    if check_file_exist(&downloading_file)? {
        println!("{} already exists.", downloading_file.display(),);
    } else {
        println!(
            "Created {} successfully as an empty file.",
            downloading_file.display()
        )
    }

    let _need_to_download = check_file(&downloading_file, &config)?;

    Ok(())
}
