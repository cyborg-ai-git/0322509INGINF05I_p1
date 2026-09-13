//! Prelievi reali su quattro PTY e prove dell'oracolo su log deliberatamente alterati.
use atm_test_support::{lab::Lab, verify};
use serde_json::{Value, json};
use std::{fs, path::Path, sync::OnceLock};
use tempfile::TempDir;

/// Un'interruzione deve chiudere tutti i figli anche prima del primo giro completo.
#[test]
fn interruzione_arresta_tutti_i_processi_del_laboratorio() {
    let root = tempfile::tempdir().unwrap();
    let lab = Lab::start_with_binary(root.path(), Path::new(env!("CARGO_BIN_EXE_app_atm")), 1000)
        .unwrap();
    lab.request_stop();
    lab.monitor();
    assert_eq!(lab.status()["status"], "failed");
    assert!(lab.log_dir.join("shutdown.json").is_file());
    for session in &lab.sessions {
        assert!(!session.running().unwrap());
        assert_eq!(session.snapshot(0).unwrap()["eof"], true);
    }
}

/// Un nodo terminato dopo i primi giri non deve essere nascosto da un audit valido.
#[test]
fn nodo_terminato_dopo_il_prefisso_fa_fallire_il_laboratorio() {
    let root = tempfile::tempdir().unwrap();
    let lab =
        Lab::start_with_binary(root.path(), Path::new(env!("CARGO_BIN_EXE_app_atm")), 0).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "Giri iniziali non completati"
        );
        if let Ok((events, _)) = verify::read_events(&lab.log_dir)
            && events.iter().any(|event| {
                event["atm"] == "ATM1"
                    && event["event"] == "token_received"
                    && event["hop"].as_u64().is_some_and(|hop| hop >= 16)
            })
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    lab.sessions[0].stop().unwrap();
    lab.monitor();
    assert_eq!(lab.status()["status"], "failed");
    assert!(
        lab.status()["error"]
            .as_str()
            .unwrap()
            .contains("ATM terminato")
    );
    for session in &lab.sessions {
        assert!(!session.running().unwrap());
    }
}

/// Esegue una sola demo reale; ogni prova modifica poi una propria copia isolata.
fn fixture() -> TempDir {
    // Conserva soltanto i byte: la directory della prova iniziale viene così eliminata.
    static BASE: OnceLock<Vec<(std::ffi::OsString, Vec<u8>)>> = OnceLock::new();
    let base = BASE.get_or_init(|| {
        let root = tempfile::tempdir().unwrap();
        let lab = Lab::start_with_binary(root.path(), Path::new(env!("CARGO_BIN_EXE_app_atm")), 0)
            .unwrap();
        lab.monitor();
        assert_eq!(
            lab.status()["result"]["final_balance"],
            400,
            "{}",
            lab.status()
        );
        fs::read_dir(&lab.log_dir)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect()
    });
    let copy = tempfile::tempdir().unwrap();
    for (name, bytes) in base {
        fs::write(copy.path().join(name), bytes).unwrap();
    }
    copy
}

fn change(root: &Path, node: u8, mutation: impl FnOnce(&mut Vec<Value>)) {
    let path = root.join(format!("atm{node}.jsonl"));
    let raw = fs::read_to_string(&path).unwrap();
    let mut records: Vec<Value> = raw
        .split_inclusive('\n')
        .filter(|s| s.ends_with('\n'))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    mutation(&mut records);
    let text: String = records.iter().map(|v| format!("{v}\n")).collect();
    fs::write(path, text).unwrap();
}

#[test]
fn quattro_pty_reali_saldo_400_e_tre_giri_verificati() {
    let root = fixture();
    let proof = verify::verify(root.path(), 12, true).unwrap();
    assert_eq!(proof["final_balance"], 400);
    assert_eq!(proof["checked_rounds"], 3);
    let ids: Vec<Value> =
        serde_json::from_slice(&fs::read(root.path().join("launch.json")).unwrap()).unwrap();
    for key in ["pid", "pty", "session"] {
        assert_eq!(
            ids.iter()
                .map(|v| v[key].to_string())
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4
        );
    }
    println!("Scenario verificato: quattro ATM, tre giri completi, saldo finale 400.");
}

#[test]
fn saldo_alterato_rifiutato() {
    let root = fixture();
    change(root.path(), 2, |events| {
        events
            .iter_mut()
            .find(|e| e["event"] == "transaction_finished")
            .unwrap()["balance"] = json!(801);
    });
    assert!(verify::verify(root.path(), 12, true).is_err());
}

/// Il saldo corretto nei log non basta se la risorsa condivisa è stata alterata.
#[test]
fn file_del_conto_alterato_rifiutato() {
    let root = fixture();
    fs::write(root.path().join("shared.txt"), "401\n").unwrap();
    assert!(verify::verify(root.path(), 12, true).is_err());
}

#[test]
fn lacuna_nella_sequenza_rifiutata() {
    let root = fixture();
    change(root.path(), 3, |events| {
        events.remove(1);
    });
    assert!(verify::verify(root.path(), 12, true).is_err());
}

#[test]
fn sezione_critica_fuori_visita_rifiutata() {
    let root = fixture();
    change(root.path(), 3, |events| {
        events
            .iter_mut()
            .find(|e| e["event"] == "transaction_started")
            .unwrap()["unix_ns"] = json!("1");
    });
    assert!(verify::verify(root.path(), 12, true).is_err());
}

#[test]
fn pid_misto_rifiutato() {
    let root = fixture();
    change(root.path(), 1, |events| {
        events[1]["pid"] = json!(0);
    });
    assert!(verify::verify(root.path(), 12, true).is_err());
}

#[test]
fn importo_alterato_rifiutato() {
    let root = fixture();
    change(root.path(), 2, |events| {
        events
            .iter_mut()
            .find(|e| e["event"] == "transaction_finished")
            .unwrap()["detail"]["outcome"]["transaction"]["amount"] = json!(201);
    });
    assert!(verify::verify(root.path(), 12, true).is_err());
}

#[test]
fn token_rifiutato_non_nascosto_dall_arresto() {
    let root = fixture();
    change(root.path(), 4, |events| {
        let last = events.last_mut().unwrap();
        last["event"] = json!("token_rejected");
    });
    assert!(verify::verify(root.path(), 12, true).is_err());
}

#[test]
fn record_completo_non_json_rifiutato() {
    use std::io::Write;
    let root = fixture();
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(root.path().join("atm1.jsonl"))
        .unwrap();
    writeln!(file, "\nNON_JSON").unwrap();
    assert!(verify::verify(root.path(), 12, true).is_err());
}

#[test]
fn frame_incompleto_ammesso_solo_dopo_arresto_documentato() {
    let root = fixture();
    let shutdown: Value =
        serde_json::from_slice(&fs::read(root.path().join("shutdown.json")).unwrap()).unwrap();
    change(root.path(), 4, |events| {
        let mut last = events.last().unwrap().clone();
        last["seq"] = json!(last["seq"].as_u64().unwrap() + 1);
        last["event"] = json!("message_rejected");
        last["unix_ns"] = shutdown["requested_unix_ns"].clone();
        last["detail"] = json!({"reason":"messaggio incompleto: manca la newline finale"});
        events.push(last);
    });
    assert!(verify::verify(root.path(), 12, true).is_ok());
    change(root.path(), 4, |events| {
        events.last_mut().unwrap()["unix_ns"] = json!("0");
    });
    assert!(verify::verify(root.path(), 12, true).is_err());
}
