use serde::{Deserialize, Serialize};

use crate::constants::{CHUNK_SIZE, DEFAULT_PAGE_SIZE};

#[derive(Serialize, Deserialize, Debug)]
pub struct FileChunk {
    pub chunk_id: usize,
    pub hash: String,
    pub offset: u64,
    pub length: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileConfig {
    pub file_name: String,
    pub total_length: u64,
    pub total_hash: String,
    pub chunks: Vec<FileChunk>,
}

//output an iterator over (start_offset, length)
pub fn make_plan(file_length: u64) -> impl Iterator<Item = (u64, usize)> {
    let full_chunks = file_length / CHUNK_SIZE as u64;
    let full_chunks_used = full_chunks.checked_sub(1).unwrap_or_default();

    let tail_1_offset = full_chunks_used * CHUNK_SIZE as u64;

    let remain_bytes = file_length - tail_1_offset;
    let remain_pages = remain_bytes / DEFAULT_PAGE_SIZE as u64;

    let tail_1_len = if remain_bytes > CHUNK_SIZE as u64 {
        remain_pages.div_ceil(2) * DEFAULT_PAGE_SIZE as u64
    } else {
        0
    };

    let tail_2_offset = tail_1_len + tail_1_offset;
    let tail_2_len = file_length - tail_2_offset;

    (0..full_chunks_used)
        .map(|x| (x * CHUNK_SIZE as u64, CHUNK_SIZE))
        .chain(std::iter::once((tail_1_offset, tail_1_len as usize)).filter(|(_, len)| *len > 0))
        .chain(std::iter::once((tail_2_offset, tail_2_len as usize)))
}

// .map(|(offset, len)| (offset as usize, len))
#[cfg(test)]
mod test {
    use crate::plan::make_plan as make_plan_u64;
    const M: usize = 1024 * 1024;
    const K: usize = 1024;

    fn make_plan_usize(file_length: usize) -> impl Iterator<Item = (u64, usize)> {
        make_plan_u64(file_length as u64)
    }

    #[test]
    fn test_make_plan() {
        // Case 1,   file_length <= 32MiB
        assert_eq!(
            vec![(0, 17_245_233)],
            make_plan_usize(17_245_233).collect::<Vec<_>>()
        );

        assert_eq!(
            vec![(0, 32 * M)],
            make_plan_usize(32 * M).collect::<Vec<_>>()
        );

        //Case 2,  32MiB < file_length <= 64MiB

        assert_eq!(
            vec![
                (0, 24 * M + 612 * K), // aligned to 4K
                (24 * M + 612 * K, 24 * M + 609 * K + 343)
            ],
            make_plan_usize(49 * M + 197 * K + 343)
                .map(|(offset, len)| (offset as usize, len))
                .collect::<Vec<_>>()
        );

        assert_eq!(
            vec![(0, 32 * M), (32 * M, 32 * M)],
            make_plan_usize(64 * M)
                .map(|(offset, len)| (offset as usize, len))
                .collect::<Vec<_>>()
        );

        // Case 3, file_length > 64 MiB

        assert_eq!(
            vec![
                (0, 32 * M),
                (32 * M, 16 * M + 52 * K), // aligned to 4K
                (48 * M + 52 * K, 16 * M + 48 * K)
            ],
            make_plan_usize(64 * M + 100 * K)
                .map(|(offset, len)| (offset as usize, len))
                .collect::<Vec<_>>()
        );

        assert_eq!(
            vec![
                (0, 32 * M),
                (32 * M, 32 * M),
                (64 * M, 32 * M), // aligned to 4K
                (96 * M, 32 * M - 1),
            ],
            make_plan_usize(128 * M - 1)
                .map(|(offset, len)| (offset as usize, len))
                .collect::<Vec<_>>()
        );

        assert_eq!(
            vec![
                (0, 32 * M),
                (32 * M, 32 * M),
                (64 * M, 32 * M),
                (96 * M, 16 * M),
                (112 * M, 16 * M + 1)
            ],
            make_plan_usize(128 * M + 1)
                .map(|(offset, len)| (offset as usize, len))
                .collect::<Vec<_>>()
        );
    }
}
