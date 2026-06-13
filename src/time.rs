//! System-clock helpers shared across hook dispatch and TUI refresh.

use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock seconds since the Unix epoch, with `0` as the fallback
/// when the clock is earlier than the epoch (practically impossible,
/// but the `Result` shape forces us to handle it).
pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secs_reads_wall_clock() {
        let secs = now_epoch_secs();
        assert!(secs > 1_000_000_000);
    }
}
