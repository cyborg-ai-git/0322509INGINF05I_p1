//! Log indipendente per processo, destinato all’osservazione esterna.

use super::BUdpLog;
use serde_json::{Value, json};
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Registrazione indipendente degli eventi di un processo.
///
/// Nessun ATM legge i file di audit per coordinarsi. Le copie UDP sono soltanto
/// visualizzate; i file e il lock locale della console non concedono accesso
/// al conto e non partecipano alla mutua esclusione distribuita.
pub struct BLog {
    atm: String,
    sequence: u64,
    json: bool,
    audit: Option<std::fs::File>,
    console_hop_limit: Option<u64>,
    udp: Option<BUdpLog>,
}
impl BLog {
    /// Crea un logger con identificativo ATM e formato di console selezionato.
    ///
    /// La sequenza locale parte da zero e viene incrementata prima di ogni evento.
    /// `json = true` seleziona JSONL; altrimenti la console usa etichette italiane.
    pub fn new(atm: &str, json: bool) -> Self {
        Self {
            atm: atm.into(),
            sequence: 0,
            json,
            audit: None,
            console_hop_limit: None,
            udp: None,
        }
    }
    /// Collega la diffusione UDP; i log remoti non modificano il conto o il token.
    pub fn with_udp(mut self, udp: BUdpLog) -> Self {
        self.udp = Some(udp);
        self
    }

    /// Apre un nuovo file di audit dedicato al nodo; `None` lascia il logger invariato.
    ///
    /// # Errori
    /// Propaga gli errori di creazione, inclusa l’esistenza del file. Le evidenze
    /// precedenti non vengono sovrascritte e nessun ATM rilegge questo file.
    pub fn with_audit_file(mut self, path: Option<&std::path::Path>) -> io::Result<Self> {
        if let Some(path) = path {
            self.audit = Some(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)?,
            );
        }
        Ok(self)
    }

    /// Limita la sola vista di console; il file di audit riceve comunque tutti gli eventi.
    ///
    /// La CLI richiede un file di audit quando attiva questo limite. Gli eventi
    /// senza hop, come l’avvio, vengono sempre mostrati.
    pub fn with_console_hop_limit(mut self, limit: Option<u64>) -> Self {
        self.console_hop_limit = limit;
        self
    }

    /// Scrive un evento con sequenza locale, PID e istante in nanosecondi Unix.
    ///
    /// Il file di audit è scritto prima della console. Il flush rende disponibili i
    /// dati all’osservatore ma non equivale a `fsync` né a durabilità del conto.
    /// Il lock di stdout è esclusivamente locale e non coordina i quattro ATM.
    ///
    /// # Errori
    /// Propaga overflow della sequenza, orologio anteriore all’epoca Unix ed errori
    /// di scrittura o flush. Il chiamante deve arrestarsi senza continuare la transazione.
    pub fn event(
        &mut self,
        event: &str,
        hop: Option<u64>,
        balance: Option<i64>,
        detail: Value,
    ) -> io::Result<()> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("sequenza del log esaurita"))?;
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?;
        let record = json!({"atm": self.atm, "pid": std::process::id(), "seq": self.sequence,
            "unix_ns": time.as_nanos().to_string(), "event": event, "hop": hop, "balance": balance, "detail": detail});
        if let Some(audit) = &mut self.audit {
            writeln!(audit, "{record}")?;
            audit.flush()?;
        }
        // La telemetria UDP è best effort: un suo errore non ricrea né trattiene il token.
        if let Some(udp) = &self.udp
            && let Err(error) = udp.publish(&record)
        {
            eprintln!("Avviso log UDP: {error}");
        }
        if self
            .console_hop_limit
            .is_some_and(|limit| hop.is_some_and(|h| h > limit))
        {
            return Ok(());
        }
        let stdout = io::stdout();
        let mut output = stdout.lock();
        if self.json {
            writeln!(output, "{record}")?;
        } else {
            let h = hop.map_or_else(|| "-".into(), |v| v.to_string());
            let b = balance.map_or_else(|| "-".into(), |v| v.to_string());
            let label = event_label(event);
            writeln!(output, "[{}] {label}  hop={h}  saldo={b}", self.atm)?;
            if let Some(outcome) = detail.get("outcome") {
                let status = if outcome["status"] == "Completed" {
                    "COMPLETATA".to_string()
                } else {
                    format!(
                        "RIFIUTATA: {}",
                        outcome["status"]["Rejected"]["reason"]
                            .as_str()
                            .unwrap_or("motivo non disponibile")
                    )
                };
                writeln!(
                    output,
                    "  {} {}: {} -> {} ({})",
                    operation_label(&outcome["transaction"]["kind"]),
                    outcome["transaction"]["amount"],
                    outcome["previous_balance"],
                    outcome["new_balance"],
                    status
                )?;
            } else if event == "transaction_started" {
                writeln!(
                    output,
                    "  {} {}",
                    operation_label(&detail["transaction"]["kind"]),
                    detail["transaction"]["amount"]
                )?;
            } else if event == "node_ready" {
                writeln!(
                    output,
                    "  ascolto={} successore={}",
                    detail["bind"].as_str().unwrap_or("-"),
                    detail["successor"].as_str().unwrap_or("-")
                )?;
                writeln!(
                    output,
                    "  PID={} richieste in coda={}",
                    detail["pid"], detail["pending"]
                )?;
            } else if event.ends_with("rejected") {
                writeln!(
                    output,
                    "  motivo: {}",
                    detail["reason"]
                        .as_str()
                        .unwrap_or("motivo non disponibile")
                )?;
            }
        }
        output.flush()
    }
}

// Traduce soltanto la vista umana; lo schema JSON mantiene gli identificatori stabili.
fn operation_label(kind: &Value) -> &str {
    match kind.as_str() {
        Some("Deposit") => "Deposito",
        Some("Withdrawal") => "Prelievo",
        _ => "Operazione sconosciuta",
    }
}

/// Etichette italiane comuni alla console locale e alle copie UDP.
pub(crate) fn event_label(event: &str) -> &str {
    match event {
        "node_ready" => "PRONTO",
        "token_created" => "TOKEN CREATO",
        "token_received" => "TOKEN RICEVUTO",
        "token_forwarding" => "INVIO TOKEN",
        "token_forwarded" => "TOKEN INVIATO",
        "transaction_started" => "INIZIO TRANSAZIONE",
        "transaction_finished" => "FINE TRANSAZIONE",
        "message_rejected" => "MESSAGGIO RIFIUTATO",
        "token_rejected" => "TOKEN RIFIUTATO",
        _ => event,
    }
}
