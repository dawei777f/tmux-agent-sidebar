use tmux_agent_sidebar::cli::capture::ansi::parse_ansi;
use tmux_agent_sidebar::cli::capture::tmux_probe::PaneGeom;

#[test]
fn pane_geom_parses_tmux_format_line() {
    let line = "%1,0,0,80,40,1";
    let pane = PaneGeom::parse(line).unwrap();
    assert_eq!(pane.pane_id, "%1");
    assert_eq!(pane.left, 0);
    assert_eq!(pane.top, 0);
    assert_eq!(pane.width, 80);
    assert_eq!(pane.height, 40);
    assert!(pane.active);
}

#[test]
fn parse_ansi_emits_cell_with_256_color() {
    // "\x1b[38;5;117mhi" -> two cells 'h' and 'i' with fg=117
    let bytes = b"\x1b[38;5;117mhi";
    let grid = parse_ansi(bytes, 4, 1);
    assert_eq!(grid.len(), 1);
    assert_eq!(grid[0].len(), 4);
    assert_eq!(grid[0][0].ch, 'h');
    assert_eq!(grid[0][0].fg, Some(117));
    assert_eq!(grid[0][1].ch, 'i');
    assert_eq!(grid[0][1].fg, Some(117));
    assert_eq!(grid[0][2].ch, ' ');
    assert_eq!(grid[0][2].fg, None);
}

#[test]
fn parse_ansi_handles_newlines_as_row_advance() {
    let bytes = b"ab\r\ncd";
    let grid = parse_ansi(bytes, 2, 2);
    assert_eq!(grid[0][0].ch, 'a');
    assert_eq!(grid[0][1].ch, 'b');
    assert_eq!(grid[1][0].ch, 'c');
    assert_eq!(grid[1][1].ch, 'd');
}

#[test]
fn parse_ansi_honors_reset_sgr() {
    let bytes = b"\x1b[38;5;117ma\x1b[0mb";
    let grid = parse_ansi(bytes, 4, 1);
    assert_eq!(grid[0][0].fg, Some(117));
    assert_eq!(grid[0][1].fg, None);
}

#[test]
fn parse_ansi_sample_pane_snapshot() {
    let bytes = std::fs::read("tests/fixtures/capture/sample-pane.ansi").unwrap();
    let grid = parse_ansi(&bytes, 40, 5);
    insta::assert_debug_snapshot!(grid);
}

#[test]
fn canvas_assembles_two_panes_side_by_side_with_border() {
    use tmux_agent_sidebar::cli::capture::ansi::StyledCell;
    use tmux_agent_sidebar::cli::capture::canvas::{PaneContent, WindowGeom, assemble};
    use tmux_agent_sidebar::cli::capture::tmux_probe::PaneGeom;

    let left_pane = PaneGeom {
        pane_id: "%1".into(),
        left: 0,
        top: 0,
        width: 4,
        height: 2,
        active: true,
    };
    let right_pane = PaneGeom {
        pane_id: "%2".into(),
        left: 5,
        top: 0,
        width: 4,
        height: 2,
        active: false,
    };

    let make = |ch: char| {
        vec![
            vec![
                StyledCell {
                    ch,
                    ..Default::default()
                };
                4
            ];
            2
        ]
    };

    let panes = vec![
        PaneContent {
            geom: left_pane,
            cells: make('L'),
        },
        PaneContent {
            geom: right_pane,
            cells: make('R'),
        },
    ];

    let geom = WindowGeom { cols: 9, rows: 2 };
    let canvas = assemble(&geom, &panes);

    let rendered: String = canvas
        .iter()
        .map(|row| row.iter().map(|c| c.ch).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(rendered, @r"
    LLLL│RRRR
    LLLL│RRRR
    ");
}

#[test]
fn canvas_assembles_2x2_grid_resolves_center_cross() {
    use tmux_agent_sidebar::cli::capture::ansi::StyledCell;
    use tmux_agent_sidebar::cli::capture::canvas::{PaneContent, WindowGeom, assemble};
    use tmux_agent_sidebar::cli::capture::tmux_probe::PaneGeom;

    // 4 panes in a 2x2 grid, each 2x2, with 1-cell borders at col=2 and row=2
    // Window is 5x5: panes at (0,0),(3,0),(0,3),(3,3)
    let make_pane = |id: &str, left: u16, top: u16, ch: char| -> PaneContent {
        PaneContent {
            geom: PaneGeom {
                pane_id: id.into(),
                left,
                top,
                width: 2,
                height: 2,
                active: false,
            },
            cells: vec![
                vec![
                    StyledCell {
                        ch,
                        ..Default::default()
                    };
                    2
                ];
                2
            ],
        }
    };

    let panes = vec![
        make_pane("%1", 0, 0, 'A'),
        make_pane("%2", 3, 0, 'B'),
        make_pane("%3", 0, 3, 'C'),
        make_pane("%4", 3, 3, 'D'),
    ];

    let geom = WindowGeom { cols: 5, rows: 5 };
    let canvas = assemble(&geom, &panes);

    let rendered: String = canvas
        .iter()
        .map(|row| row.iter().map(|c| c.ch).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(rendered, @r"
    AA│BB
    AA│BB
    ──┼──
    CC│DD
    CC│DD
    ");
}

#[test]
fn render_html_emits_pre_with_per_cell_spans() {
    use tmux_agent_sidebar::cli::capture::ansi::StyledCell;
    use tmux_agent_sidebar::cli::capture::render_html::render_html;

    let cells = vec![vec![
        StyledCell {
            ch: 'a',
            fg: Some(117),
            ..Default::default()
        },
        StyledCell {
            ch: 'b',
            fg: Some(180),
            bold: true,
            ..Default::default()
        },
    ]];
    let html = render_html(&cells);
    insta::assert_snapshot!(html);
}

#[test]
#[ignore = "requires local rmux"]
fn capture_frames_sequence_integration() {
    use tmux_agent_sidebar::cli;

    let session = unique_session_name("cap-seq");
    create_capture_session(&session);
    let _cleanup = scopeguard::guard(session.clone(), |session| kill_capture_session(&session));

    send_text_to_session(&session, "printf 'hi\\n'");
    std::thread::sleep(std::time::Duration::from_millis(100));

    let tmp = tempfile::tempdir().unwrap();
    let code = cli::run(&[
        "capture".into(),
        "--session".into(),
        session.clone(),
        "--frames-out".into(),
        tmp.path().to_string_lossy().into(),
        "--duration-ms".into(),
        "300".into(),
        "--fps".into(),
        "10".into(),
    ])
    .unwrap();
    assert_eq!(code, 0);

    assert!(tmp.path().join("frame0000.html").exists());
    assert!(tmp.path().join("frame0002.html").exists());
    let manifest_path = tmp.path().join("manifest.json");
    assert!(manifest_path.exists());

    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["fps"], 10);
    assert_eq!(manifest["duration_ms"], 300);
    assert_eq!(manifest["frame_count"], 3);
}

#[test]
#[ignore = "requires local rmux"]
fn capture_single_frame_integration() {
    use tmux_agent_sidebar::cli;

    let session = unique_session_name("cap-it");
    create_capture_session(&session);
    let _cleanup = scopeguard::guard(session.clone(), |session| kill_capture_session(&session));

    send_text_to_session(&session, "printf '\\033[38;5;117mhi\\n'");
    std::thread::sleep(std::time::Duration::from_millis(200));

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("frame.html");
    // --window omitted: capture defaults to the session's active window,
    // sidestepping any `base-index` setting the developer's tmux may have.
    let code = cli::run(&[
        "capture".into(),
        "--session".into(),
        session,
        "--frame-out".into(),
        out.to_string_lossy().into(),
    ])
    .unwrap();
    assert_eq!(code, 0);
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(html.contains("<pre"));
}

fn unique_session_name(prefix: &str) -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis();
    format!("{prefix}-{}-{millis}", std::process::id())
}

fn create_capture_session(session: &str) {
    let mut conn = rmux_connection();
    let session_name = rmux_proto::SessionName::new(session).expect("valid session name");
    let response = conn
        .new_session(
            session_name,
            true,
            Some(rmux_proto::TerminalSize { cols: 40, rows: 10 }),
        )
        .expect("create rmux session");
    match response {
        rmux_proto::Response::NewSession(_) => {}
        rmux_proto::Response::Error(error) => panic!("new-session failed: {}", error.error),
        other => panic!("new-session returned {}", other.command_name()),
    }
}

fn kill_capture_session(session: &str) {
    let Some(session_name) = rmux_proto::SessionName::new(session).ok() else {
        return;
    };
    if let Ok(mut conn) = try_rmux_connection() {
        let _ = conn.kill_session(rmux_proto::KillSessionRequest {
            target: session_name,
            kill_all_except_target: false,
            clear_alerts: false,
        });
    }
}

fn send_text_to_session(session: &str, text: &str) {
    let pane_id =
        tmux_agent_sidebar::tmux::list_panes_formatted(Some(session), false, "#{pane_id}")
            .expect("list rmux panes")
            .lines()
            .next()
            .and_then(|pane| pane.trim().strip_prefix('%'))
            .and_then(|pane| pane.parse::<u32>().ok())
            .expect("parse first pane id");
    let session = rmux_proto::SessionName::new(session).expect("valid session name");
    tokio_runtime().block_on(async move {
        let rmux = rmux_sdk::Rmux::builder()
            .default_timeout(std::time::Duration::from_secs(5))
            .connect()
            .await
            .expect("connect rmux sdk");
        let pane = rmux
            .pane_by_id(session, rmux_proto::PaneId::new(pane_id))
            .await
            .expect("resolve rmux pane by id");
        rmux.broadcast(std::slice::from_ref(&pane), rmux_sdk::Input::Text(text))
            .await
            .expect("broadcast text");
        rmux.broadcast(std::slice::from_ref(&pane), rmux_sdk::Input::Key("Enter"))
            .await
            .expect("broadcast enter");
    });
}

fn rmux_connection() -> rmux_client::Connection {
    try_rmux_connection().expect("connect to rmux server")
}

fn try_rmux_connection() -> Result<rmux_client::Connection, String> {
    let socket_path =
        rmux_client::resolve_socket_path(None, None).map_err(|err| err.to_string())?;
    rmux_client::Connection::start_server(
        &socket_path,
        false,
        rmux_client::AutoStartConfig::disabled(),
    )
    .map_err(|err| format!("{err:?}"))
}

fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime")
    })
}
