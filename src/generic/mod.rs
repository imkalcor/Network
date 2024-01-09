use std::time::{SystemTime, UNIX_EPOCH};

pub mod events;

pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}
