use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::port::PaneProcessSnapshot;
use crate::tmux::{self, SessionInfo};

const PROTOCOL_VERSION: u16 = 1;
const SNAPSHOT_TTL: Duration = Duration::from_millis(900);
const PORT_SCAN_TTL: Duration = Duration::from_secs(10);
const BG_SHELL_SWEEP_TTL: Duration = Duration::from_secs(5);
const OWNER_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const DAEMON_IDLE_TTL: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(150);
const READ_TIMEOUT: Duration = Duration::from_millis(700);
const SIDEBAR_DAEMON_ADDR: &str = "@sidebar_daemon_addr";

static DAEMON_ADDR_CACHE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GlobalSnapshot {
    pub(crate) sessions: Vec<SessionInfo>,
    pub(crate) port_snapshot: Option<PaneProcessSnapshot>,
    pub(crate) port_snapshot_fresh: bool,
    pub(crate) captured_at_epoch_ms: u128,
}

#[derive(Clone, Copy)]
pub(crate) struct SnapshotRefresh {
    include_ports: bool,
    sweep_bg_shells: bool,
}

#[derive(Clone)]
pub(crate) struct SnapshotProvider {
    refresh: Arc<dyn Fn(SnapshotRefresh) -> GlobalSnapshot + Send + Sync>,
}

impl SnapshotProvider {
    pub(crate) fn new(
        refresh: impl Fn(SnapshotRefresh) -> GlobalSnapshot + Send + Sync + 'static,
    ) -> Self {
        Self {
            refresh: Arc::new(refresh),
        }
    }

    fn refresh(&self, refresh: SnapshotRefresh) -> GlobalSnapshot {
        (self.refresh)(refresh)
    }
}

impl Default for SnapshotProvider {
    fn default() -> Self {
        Self::new(capture_global_snapshot)
    }
}

#[derive(Default)]
struct SharedSnapshot {
    snapshot: Option<CachedSnapshot>,
    port_snapshot: Option<CachedPortSnapshot>,
    last_bg_shell_sweep: Option<Instant>,
}

struct CachedSnapshot {
    captured_at: Instant,
    value: GlobalSnapshot,
}

struct CachedPortSnapshot {
    captured_at: Instant,
    value: Option<PaneProcessSnapshot>,
}

#[derive(Serialize, Deserialize)]
struct SnapshotRequest {
    version: u16,
    kind: RequestKind,
}

#[derive(Serialize, Deserialize)]
enum RequestKind {
    Snapshot,
}

#[derive(Serialize, Deserialize)]
struct SnapshotResponse {
    version: u16,
    snapshot: GlobalSnapshot,
}

pub(crate) fn capture_global_snapshot(refresh: SnapshotRefresh) -> GlobalSnapshot {
    let (mut sessions, mut process_snapshot) = tmux::query_sessions_with_process_snapshot();
    if refresh.sweep_bg_shells {
        crate::state::sweep_dead_bg_shells(&mut sessions, &mut process_snapshot);
    }
    let port_snapshot = if refresh.include_ports {
        crate::port::scan_session_process_snapshot(&sessions, process_snapshot.as_ref())
    } else {
        None
    };
    GlobalSnapshot {
        sessions,
        port_snapshot,
        port_snapshot_fresh: refresh.include_ports,
        captured_at_epoch_ms: now_epoch_ms(),
    }
}

pub(crate) fn snapshot_from_daemon() -> Option<GlobalSnapshot> {
    request_snapshot_from_tmux_daemon().or_else(|| {
        start_daemon_from_current_exe()?;
        request_snapshot_from_tmux_daemon()
    })
}

fn request_snapshot_from_tmux_daemon() -> Option<GlobalSnapshot> {
    if let Some(addr) = cached_daemon_addr() {
        if let Some(snapshot) = request_snapshot(&addr) {
            return Some(snapshot);
        }
        set_cached_daemon_addr(None);
    }

    let addr = tmux::get_option(SIDEBAR_DAEMON_ADDR)?;
    let snapshot = request_snapshot(&addr)?;
    set_cached_daemon_addr(Some(addr));
    Some(snapshot)
}

fn request_snapshot(addr: &str) -> Option<GlobalSnapshot> {
    let socket_addr = addr.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&socket_addr, CONNECT_TIMEOUT).ok()?;
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));

    let req = SnapshotRequest {
        version: PROTOCOL_VERSION,
        kind: RequestKind::Snapshot,
    };
    serde_json::to_writer(&mut stream, &req).ok()?;
    stream.write_all(b"\n").ok()?;
    stream.flush().ok()?;

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line).ok()?;
    let response: SnapshotResponse = serde_json::from_str(&line).ok()?;
    (response.version == PROTOCOL_VERSION).then_some(response.snapshot)
}

pub(crate) fn run_daemon_from_cli() -> i32 {
    match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => {
            let addr = match listener.local_addr() {
                Ok(addr) => addr,
                Err(err) => {
                    eprintln!("tmux-agent-sidebar daemon failed to read addr: {err}");
                    return 1;
                }
            };
            let _ = tmux::run_tmux(&["set", "-g", SIDEBAR_DAEMON_ADDR, &addr.to_string()]);
            run_registered_listener(listener, SnapshotProvider::default(), addr.to_string());
            0
        }
        Err(err) => {
            eprintln!("tmux-agent-sidebar daemon failed to bind: {err}");
            1
        }
    }
}

fn start_daemon_from_current_exe() -> Option<()> {
    let exe = std::env::current_exe().ok()?;
    let child = Command::new(exe)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let _ = child.id();
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(25));
        if request_snapshot_from_tmux_daemon().is_some() {
            return Some(());
        }
    }
    None
}

fn run_registered_listener(listener: TcpListener, provider: SnapshotProvider, addr: String) {
    run_listener_inner(listener, provider, Some(addr));
}

fn run_listener_inner(
    listener: TcpListener,
    provider: SnapshotProvider,
    registered_addr: Option<String>,
) {
    run_listener_until_idle(listener, provider, registered_addr, DAEMON_IDLE_TTL, None);
}

fn run_listener_until_idle(
    listener: TcpListener,
    provider: SnapshotProvider,
    registered_addr: Option<String>,
    idle_ttl: Duration,
    max_clients: Option<usize>,
) {
    if listener.set_nonblocking(true).is_err() {
        return;
    }
    let shared = Arc::new(Mutex::new(SharedSnapshot::default()));
    let mut last_client_at = Instant::now();
    let mut last_owner_check = Instant::now();
    let mut accepted_clients = 0usize;

    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                last_client_at = Instant::now();
                accepted_clients += 1;
                let shared = Arc::clone(&shared);
                let provider = provider.clone();
                std::thread::spawn(move || {
                    let _ = handle_client(stream, &shared, &provider);
                });
                if max_clients.is_some_and(|max| accepted_clients >= max) {
                    return;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => {}
        }

        if let Some(addr) = &registered_addr
            && last_owner_check.elapsed() >= OWNER_CHECK_INTERVAL
        {
            if tmux::get_option(SIDEBAR_DAEMON_ADDR).as_deref() != Some(addr.as_str()) {
                return;
            }
            last_owner_check = Instant::now();
        }

        if last_client_at.elapsed() >= idle_ttl {
            return;
        }

        std::thread::sleep(Duration::from_millis(25));
    }
}

fn cached_daemon_addr() -> Option<String> {
    DAEMON_ADDR_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|addr| addr.clone())
}

fn set_cached_daemon_addr(addr: Option<String>) {
    if let Ok(mut cached) = DAEMON_ADDR_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *cached = addr;
    }
}

fn handle_client(
    mut stream: TcpStream,
    shared: &Arc<Mutex<SharedSnapshot>>,
    provider: &SnapshotProvider,
) -> std::io::Result<()> {
    let mut line = String::new();
    let mut reader = BufReader::new(stream.try_clone()?);
    reader.read_line(&mut line)?;
    let Ok(req) = serde_json::from_str::<SnapshotRequest>(&line) else {
        return Ok(());
    };
    if req.version != PROTOCOL_VERSION {
        return Ok(());
    }
    match req.kind {
        RequestKind::Snapshot => {
            let snapshot = cached_snapshot(shared, provider);
            let response = SnapshotResponse {
                version: PROTOCOL_VERSION,
                snapshot,
            };
            serde_json::to_writer(&mut stream, &response)?;
            stream.write_all(b"\n")?;
            stream.flush()?;
        }
    }
    Ok(())
}

fn cached_snapshot(
    shared: &Arc<Mutex<SharedSnapshot>>,
    provider: &SnapshotProvider,
) -> GlobalSnapshot {
    let mut shared = shared.lock().expect("daemon snapshot lock poisoned");
    let now = Instant::now();
    if let Some(cached) = &shared.snapshot
        && now.duration_since(cached.captured_at) < SNAPSHOT_TTL
    {
        return cached.value.clone();
    }

    let ports_fresh = shared
        .port_snapshot
        .as_ref()
        .is_some_and(|cached_ports| now.duration_since(cached_ports.captured_at) < PORT_SCAN_TTL);
    let should_sweep_bg_shells = shared
        .last_bg_shell_sweep
        .is_none_or(|last| now.duration_since(last) >= BG_SHELL_SWEEP_TTL);
    let mut snapshot = provider.refresh(SnapshotRefresh {
        include_ports: !ports_fresh,
        sweep_bg_shells: should_sweep_bg_shells,
    });
    if should_sweep_bg_shells {
        shared.last_bg_shell_sweep = Some(now);
    }
    if let Some(cached_ports) = &shared.port_snapshot
        && ports_fresh
    {
        snapshot.port_snapshot = cached_ports.value.clone();
        snapshot.port_snapshot_fresh = false;
    } else {
        shared.port_snapshot = Some(CachedPortSnapshot {
            captured_at: now,
            value: snapshot.port_snapshot.clone(),
        });
        snapshot.port_snapshot_fresh = true;
    }

    shared.snapshot = Some(CachedSnapshot {
        captured_at: now,
        value: snapshot.clone(),
    });
    snapshot
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_snapshot(mark: &str) -> GlobalSnapshot {
        GlobalSnapshot {
            sessions: vec![SessionInfo {
                session_name: mark.to_string(),
                windows: Vec::new(),
            }],
            port_snapshot: Some(PaneProcessSnapshot::default()),
            port_snapshot_fresh: true,
            captured_at_epoch_ms: 42,
        }
    }

    #[test]
    fn daemon_serves_multiple_clients_from_one_collector_refresh() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test daemon");
        let addr = listener.local_addr().expect("test daemon addr").to_string();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = {
            let calls = Arc::clone(&calls);
            SnapshotProvider::new(move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                test_snapshot("shared")
            })
        };
        let server = std::thread::spawn(move || {
            run_listener_until_idle(listener, provider, None, Duration::from_secs(5), Some(2));
        });

        let first = request_snapshot(&addr).expect("first snapshot");
        let second = request_snapshot(&addr).expect("second snapshot");
        server.join().expect("server exits");

        assert_eq!(first.sessions[0].session_name, "shared");
        assert_eq!(second.sessions[0].session_name, "shared");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "two clients inside the daemon TTL must share one global refresh"
        );
    }

    #[test]
    fn cached_snapshot_keeps_port_scan_for_longer_than_base_snapshot() {
        let shared = Arc::new(Mutex::new(SharedSnapshot::default()));
        let calls = Arc::new(AtomicUsize::new(0));
        let port_scans = Arc::new(AtomicUsize::new(0));
        let provider = {
            let calls = Arc::clone(&calls);
            let port_scans = Arc::clone(&port_scans);
            SnapshotProvider::new(move |refresh| {
                let idx = calls.fetch_add(1, Ordering::SeqCst);
                let mut snapshot = test_snapshot(&format!("tick-{idx}"));
                snapshot.port_snapshot = refresh.include_ports.then(|| {
                    port_scans.fetch_add(1, Ordering::SeqCst);
                    let mut ports = PaneProcessSnapshot::default();
                    ports
                        .ports_by_pane
                        .insert("%1".into(), vec![3000 + idx as u16]);
                    ports
                });
                snapshot
            })
        };

        let first = cached_snapshot(&shared, &provider);
        {
            let mut guard = shared.lock().unwrap();
            guard.snapshot.as_mut().unwrap().captured_at =
                Instant::now() - SNAPSHOT_TTL - Duration::from_millis(1);
        }
        let second = cached_snapshot(&shared, &provider);

        assert_eq!(first.sessions[0].session_name, "tick-0");
        assert_eq!(second.sessions[0].session_name, "tick-1");
        assert_eq!(
            second
                .port_snapshot
                .unwrap()
                .ports_by_pane
                .get("%1")
                .cloned(),
            Some(vec![3000]),
            "base snapshot may refresh every second while lsof-backed port data stays cached"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            port_scans.load(Ordering::SeqCst),
            1,
            "cached port data must avoid a second lsof-backed port scan"
        );
        assert!(
            !second.port_snapshot_fresh,
            "clients can distinguish cached port data from a new liveness scan"
        );
    }
}
