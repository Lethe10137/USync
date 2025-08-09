use once_cell::sync::Lazy;
use owo_colors::*;
use tokio::time::Instant;

pub static PROGRAM_START_TIME: Lazy<Instant> = Lazy::new(Instant::now);

pub fn print_relative_time(label: &str, instant: Instant) -> f64 {
    let elapsed = instant.duration_since(*PROGRAM_START_TIME);
    let time_ms = elapsed.as_secs_f64() * 1000.0;
    println!("[{:.6}ms] {}", time_ms.red(), label.blue());
    time_ms
}
