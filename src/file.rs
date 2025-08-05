use memmap2::{Mmap, MmapOptions};
use std::fs::{File, OpenOptions};
use std::io::Result;
use std::os::unix::fs::FileExt;
use std::path::Path;

pub fn file_len<P: AsRef<Path>>(path: P) -> Result<u64> {
    Ok(std::fs::metadata(path)?.len())
}

pub fn mmap_segment<P: AsRef<Path>>(path: P, offset: u64, length: usize) -> Result<Mmap> {
    let file = File::open(path)?;

    let page_size = page_size::get() as u64;
    assert_eq!(offset % page_size, 0, "Unaligned offset!");

    let mmap = unsafe { MmapOptions::new().offset(offset).len(length).map(&file)? };

    Ok(mmap)
}

pub fn create_sparse_file<P: AsRef<Path>>(path: P, length: u64) -> Result<()> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.set_len(length)?;
    Ok(())
}

pub fn write_at<P: AsRef<Path>>(path: P, offset: u64, data: &[u8]) -> Result<()> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.write_at(data, offset)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use tempfile::tempdir;

    #[test]
    fn test_sparse_file_write_and_read() -> Result<()> {
        // UNIX only
        let dir = tempdir()?;
        let file_path = dir.path().join("sparse_test.bin");

        let file_size: u64 = 1 << 30; // 1GiB
        create_sparse_file(&file_path, file_size)?;

        let block_size: usize = 4096;

        // [0x88; 4096] at 0B offset
        let block1 = vec![0x88; block_size];
        write_at(&file_path, 0, &block1)?;

        // [0x94; 4096] at 734MiB offset
        let offset2: u64 = 734 * 1024 * 1024;
        let block2 = vec![0x94; block_size];
        write_at(&file_path, offset2, &block2)?;

        // Logical length of file = 1 GiB
        let file_length = file_len(&file_path)?;
        assert_eq!(file_length, file_size);
        println!("Logical file length: {} bytes", file_length);

        // Actual disk usage: 8 KiB
        let used_bytes = std::fs::metadata(&file_path)?.blocks() * 512;
        println!("Actual disk usage: {} bytes", used_bytes);

        assert_eq!(
            used_bytes, 8192,
            "Not a sparse file. Expected to fail on Windows for now!"
        );

        // Check content
        {
            let mmap1 = mmap_segment(&file_path, 0, block_size)?;
            let slice1 = &mmap1[0..block_size];
            assert!(slice1.iter().all(|&b| b == 0x88));
        }
        {
            let mmap2 = mmap_segment(&file_path, offset2, block_size)?;
            let page_size = page_size::get() as u64;
            let delta = (offset2 % page_size) as usize;
            let slice2 = &mmap2[delta..delta + block_size];
            assert!(slice2.iter().all(|&b| b == 0x94));
        }

        Ok(())
    }
}
