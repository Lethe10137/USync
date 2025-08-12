pub mod file;
pub mod plan;
pub mod timer;
pub mod timer_logger;

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;

pub fn unix_ms_to_tokio_instant(unix_ms: u64) -> Instant {
    // Current wall-clock time
    let now_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64;

    let now_instant = Instant::now();

    if unix_ms >= now_unix_ms {
        // Future timestamp: add the difference
        let diff = unix_ms - now_unix_ms;
        now_instant + Duration::from_millis(diff)
    } else {
        // Past timestamp: subtract the difference
        let diff = now_unix_ms - unix_ms;
        now_instant - Duration::from_millis(diff)
    }
}

pub trait Compare: Ord + Clone {
    fn cmax(&mut self, other: Self) {
        if *self < other {
            *self = other;
        }
    }
    fn cmin(&mut self, other: Self) {
        if *self > other {
            *self = other;
        }
    }
}

impl<T: Ord + Clone> Compare for T {}

pub fn generate_random(size: usize) -> Vec<u8> {
    use rand::Rng;
    let mut data: Vec<u8> = vec![0; size];
    for byte in data.iter_mut() {
        *byte = rand::rng().random();
    }
    data
}
