//! Prove del file bancario; il token resta l'unico meccanismo di esclusione fra ATM.
use app_atm::BAccount;
use std::fs;

/// La risorsa conserva ogni nuovo saldo e il bootstrap non sovrascrive un conto.
#[test]
fn inizializzazione_scrittura_e_rilettura_del_conto() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.txt");
    BAccount::initialize(&path, 1000).unwrap();
    for balance in [800, 900, 400] {
        BAccount::write(&path, balance).unwrap();
        assert_eq!(BAccount::read(&path).unwrap(), balance);
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("{balance}\n"));
    }
    assert!(BAccount::initialize(&path, 1000).is_err());
    assert_eq!(BAccount::read(&path).unwrap(), 400);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

/// Dati corrotti o incompleti non diventano un saldo valido per la transazione.
#[test]
fn file_mancante_o_contenuto_non_valido_rifiutati() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.txt");
    assert!(BAccount::read(&path).is_err());
    for content in [
        b"".as_slice(),
        b"-1",
        b"100 200",
        b"9223372036854775808",
        b"\xff",
        &[b'0'; 65],
    ] {
        fs::write(&path, content).unwrap();
        assert!(BAccount::read(&path).is_err(), "{content:?}");
    }
}

/// Gli errori di validazione non alterano il conto né creano temporanei.
#[test]
fn saldo_negativo_e_directory_assente_non_modificano_la_risorsa() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.txt");
    assert!(BAccount::initialize(&path, -1).is_err());
    assert!(!path.exists());
    BAccount::initialize(&path, i64::MAX).unwrap();
    assert!(BAccount::write(&path, -1).is_err());
    assert_eq!(BAccount::read(&path).unwrap(), i64::MAX);
    assert!(BAccount::write(&root.path().join("assente/shared.txt"), 400).is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}
