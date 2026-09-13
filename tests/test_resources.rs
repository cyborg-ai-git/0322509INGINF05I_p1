//! Verifica risorse reali e isolamento dei terminali.
use atm_test_support::terminal::Session;
use portable_pty::CommandBuilder;
use std::{
    collections::HashSet,
    thread,
    time::{Duration, Instant},
};

fn until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !condition() {
        assert!(Instant::now() < deadline, "Timeout della console");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn quattro_console_input_resize_e_arresto_selettivi() {
    let root = tempfile::tempdir().unwrap();
    // Compila una fixture Rust autonoma: il test non richiede strumenti locali esterni.
    let binary = root.path().join("echo_node");
    let compilation =
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg("--edition=2024")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/test_echo_node.rs"),
            )
            .arg("-o")
            .arg(&binary)
            .output()
            .expect("Avvio del compilatore della fixture");
    assert!(
        compilation.status.success(),
        "Compilazione della fixture: {}",
        String::from_utf8_lossy(&compilation.stderr)
    );
    let sessions: Vec<_> = (0..4)
        .map(|i| {
            let mut command = CommandBuilder::new(&binary);
            command.arg(format!("T{i}"));
            Session::spawn(
                &format!("T{i}"),
                command,
                &root.path().join(format!("{i}.log")),
            )
            .unwrap()
        })
        .collect();
    for key in ["pid", "pty", "session"] {
        assert_eq!(
            sessions
                .iter()
                .map(|s| s.identity()[key].to_string())
                .collect::<HashSet<_>>()
                .len(),
            4
        );
    }
    for (i, session) in sessions.iter().enumerate() {
        until(|| {
            session.snapshot(0).unwrap()["output"]
                .as_str()
                .unwrap()
                .contains("PRONTO")
        });
        session.input(&format!("MARCATORE_{i}_€\n")).unwrap();
        until(|| {
            session.snapshot(0).unwrap()["output"]
                .as_str()
                .unwrap()
                .contains(&format!("RISPOSTA T{i} MARCATORE_{i}_€"))
        });
    }
    for (i, session) in sessions.iter().enumerate() {
        let text = session.snapshot(0).unwrap()["output"]
            .as_str()
            .unwrap()
            .to_owned();
        for other in 0..4 {
            if other != i {
                assert!(!text.contains(&format!("MARCATORE_{other}")));
            }
        }
    }
    let original = sessions[0].size().unwrap();
    sessions[1].resize(30, 110).unwrap();
    assert_eq!(sessions[1].size().unwrap(), (30, 110));
    assert_eq!(sessions[0].size().unwrap(), original);
    assert!(sessions[1].resize(0, 0).is_err());
    assert!(sessions[0].snapshot(usize::MAX).is_err());
    // Ctrl+C attraversa il PTY del solo secondo figlio.
    sessions[1].input("\u{3}").unwrap();
    until(|| !sessions[1].running().unwrap());
    for i in [0, 2, 3] {
        assert!(sessions[i].running().unwrap());
    }
    for session in &sessions {
        session.stop().unwrap();
    }
    for session in &sessions {
        let snapshot = session.snapshot(0).unwrap();
        assert_eq!(snapshot["eof"], true);
        assert!(snapshot["output"].as_str().unwrap().contains("PRONTO"));
    }
}
