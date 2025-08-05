#![allow(dead_code)]
#![warn(unused_imports)]

mod constants;
mod file;
mod plan;
mod protocol;

use clap::Parser;
use file::file_len;
use std::io;
use std::path::PathBuf;
use zerocopy::IntoBytes;

use crate::file::mmap_segment;
use crate::plan::{FileChunk, FileConfig, make_plan};

#[derive(Parser, Debug)]
#[command(author, version, about = "A simple CLI program to build transmission plan.", long_about = None)]
struct Args {
    /// The path to the file to read.
    #[arg(short, long, value_name = "FILE")]
    file: PathBuf,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // Check if the path exists and is a file.
    if !args.file.exists() {
        eprintln!("Error: The specified file does not exist.");
        std::process::exit(1);
    }

    if !args.file.is_file() {
        eprintln!("Error: The specified path is not a file.");
        std::process::exit(1);
    }

    let file_name = args
        .file
        .file_name()
        .expect("Failed to get file name")
        .to_str()
        .expect("Non UTF-8 File name provided")
        .to_string();

    let total_length = file_len(&args.file)?;
    let mut total_hasher = blake3::Hasher::new();
    let mut chunks = vec![];

    for (chunk_id, (offset, length)) in make_plan(total_length as usize).enumerate() {
        let chunk = mmap_segment(&args.file, offset, length)?;
        let chunk_bytes = chunk.as_bytes();
        assert_eq!(chunk_bytes.len(), length);
        let hash = hex::encode(blake3::hash(chunk_bytes).as_bytes());
        total_hasher.update(chunk_bytes);

        chunks.push(FileChunk {
            chunk_id,
            hash,
            offset,
            length,
        })
    }

    let total_hash = hex::encode(total_hasher.finalize().as_bytes());

    let plan = FileConfig {
        file_name,
        total_hash,
        total_length,
        chunks,
    };

    println!("{}", toml::to_string_pretty(&plan).unwrap());

    Ok(())
}
