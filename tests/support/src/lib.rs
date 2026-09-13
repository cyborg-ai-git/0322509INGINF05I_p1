//! Supporto dei test Rust: sessioni PTY indipendenti e verifica causale dei log.
//! Questa libreria è una dipendenza di sviluppo, non serve ad avviare gli ATM.
//! Nessuna funzione concede il token o modifica il conto.
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]
#![forbid(unsafe_code)]

pub mod lab;
pub mod terminal;
pub mod verify;

/// Risultato con contesto per gli errori operativi del laboratorio.
pub type Result<T> = anyhow::Result<T>;

/// Genera un'identità imprevedibile per una singola esecuzione della console.
pub fn random_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|e| anyhow::anyhow!("Identità casuale non disponibile: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
