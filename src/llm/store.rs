use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const BASE_DIR: &str = "/tmp/tmux-agent-sidebar-names";

pub fn base_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var("TMUX_AGENT_SIDEBAR_NAMES_DIR") {
        PathBuf::from(override_dir)
    } else {
        PathBuf::from(BASE_DIR)
    }
}

fn sanitize_session_id(session_id: &str) -> String {
    session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn name_path(session_id: &str) -> PathBuf {
    base_dir().join(format!("{}.txt", sanitize_session_id(session_id)))
}

pub fn read(session_id: &str) -> Option<String> {
    fs::read_to_string(name_path(session_id))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn write(session_id: &str, name: &str) -> io::Result<()> {
    let dir = base_dir();
    fs::create_dir_all(&dir)?;
    let final_path = name_path(session_id);
    let tmp_path = dir.join(format!(".{}.tmp", sanitize_session_id(session_id)));
    fs::write(&tmp_path, name)?;
    fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

pub fn scan_all() -> HashMap<String, String> {
    let mut out = HashMap::new();
    let dir = base_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = file_stem_for_name_file(&path) else {
            continue;
        };
        if let Ok(content) = fs::read_to_string(&path) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                out.insert(stem, trimmed.to_string());
            }
        }
    }
    out
}

fn file_stem_for_name_file(path: &Path) -> Option<String> {
    if path.extension().and_then(|e| e.to_str()) != Some("txt") {
        return None;
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.starts_with('.'))
        .map(|s| s.to_string())
}

pub fn latest_mtime() -> Option<SystemTime> {
    let dir = base_dir();
    let entries = fs::read_dir(&dir).ok()?;
    entries
        .flatten()
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct TempEnv {
        _guard: std::sync::MutexGuard<'static, ()>,
        dir: PathBuf,
    }

    impl TempEnv {
        fn new() -> Self {
            let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = std::env::temp_dir().join(format!(
                "tmux-agent-sidebar-names-test-{}-{}",
                std::process::id(),
                rand_suffix()
            ));
            let _ = fs::remove_dir_all(&dir);
            // SAFETY: serialized via ENV_LOCK so no other thread is reading
            // or writing the env var while we mutate it.
            unsafe {
                std::env::set_var("TMUX_AGENT_SIDEBAR_NAMES_DIR", &dir);
            }
            TempEnv { _guard: guard, dir }
        }
    }

    impl Drop for TempEnv {
        fn drop(&mut self) {
            // SAFETY: still holding ENV_LOCK; no concurrent env access.
            unsafe {
                std::env::remove_var("TMUX_AGENT_SIDEBAR_NAMES_DIR");
            }
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn rand_suffix() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        format!("{nanos:x}")
    }

    #[test]
    fn sanitize_keeps_safe_chars_and_replaces_rest() {
        assert_eq!(sanitize_session_id("abc-123_xyz"), "abc-123_xyz");
        assert_eq!(sanitize_session_id("ses/with/slash"), "ses_with_slash");
        assert_eq!(sanitize_session_id("../evil"), "___evil");
        assert_eq!(sanitize_session_id("a b%c"), "a_b_c");
    }

    #[test]
    fn write_then_read_roundtrip() {
        let _env = TempEnv::new();
        write("sess-1", "refactor").unwrap();
        assert_eq!(read("sess-1").as_deref(), Some("refactor"));
    }

    #[test]
    fn read_trims_whitespace_and_treats_empty_as_missing() {
        let _env = TempEnv::new();
        write("sess-empty", "   \n").unwrap();
        assert!(read("sess-empty").is_none());
    }

    #[test]
    fn read_returns_none_when_file_absent() {
        let _env = TempEnv::new();
        assert!(read("nope").is_none());
    }

    #[test]
    fn scan_all_collects_valid_entries_and_skips_tmp_files() {
        let _env = TempEnv::new();
        write("sess-a", "alpha").unwrap();
        write("sess-b", "beta").unwrap();
        // Writer leaves no tmp files normally, but simulate one sitting
        // around to prove scan_all ignores hidden/tmp artifacts.
        fs::create_dir_all(base_dir()).unwrap();
        fs::write(base_dir().join(".sess-c.tmp"), "gamma").unwrap();

        let all = scan_all();
        assert_eq!(all.get("sess-a").map(String::as_str), Some("alpha"));
        assert_eq!(all.get("sess-b").map(String::as_str), Some("beta"));
        assert!(!all.contains_key(".sess-c"));
        assert!(!all.contains_key("sess-c"));
    }

    #[test]
    fn scan_all_returns_empty_when_dir_missing() {
        let _env = TempEnv::new();
        let all = scan_all();
        assert!(all.is_empty());
    }

    #[test]
    fn path_traversal_in_session_id_is_contained() {
        let _env = TempEnv::new();
        // Attempt to escape the base dir — sanitization should contain it.
        let evil = "../../etc/passwd";
        write(evil, "x").unwrap();
        let written = name_path(evil);
        assert!(
            written.starts_with(base_dir()),
            "sanitized path must stay under base_dir, got {written:?}"
        );
        assert_eq!(read(evil).as_deref(), Some("x"));
    }
}
