use clap::Parser;
use std::path::PathBuf;
use zerocopy::IntoBytes;

use usync::util::file::{mmap_segment, sanity_check};
use usync::util::plan::{FileChunk, FileConfig, make_plan};

#[derive(Parser, Debug)]
#[command(author, version, about = "A simple CLI program to build transmission plan.", long_about = None)]
struct Args {
    /// The path to the file to read.
    #[arg(short, long, value_name = "FILE")]
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let (total_length, file_name) = sanity_check(&args.file)?;

    let mut total_hasher = blake3::Hasher::new();
    let mut chunks = vec![];

    for (chunk_id, (offset, length)) in make_plan(total_length).enumerate() {
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
