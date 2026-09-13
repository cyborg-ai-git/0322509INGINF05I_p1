//! Processo ATM: bootstrap unico, ciclo sequenziale del token e log della sezione critica.

use app_atm::{
    BAccount, BLog, BUdpLog, CAtm, CConfig, CMessage, CNode, CToken, EAtm, ETransaction,
    SPEC_INITIAL_BALANCE,
};
use clap::Parser;
use serde_json::json;
use std::{io, net::SocketAddr, time::Duration};
use tokio::time::sleep;
use tokio::{io::BufReader, net::TcpStream};

/// Uno dei quattro processi ATM indipendenti, coordinati esclusivamente tramite Token Ring.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Identificativo del nodo: 1, 2, 3 o 4; sono accettati anche gli alias ATM1…ATM4.
    #[arg(long)]
    id: EAtm,
    /// Indirizzo di ascolto su 127.0.0.1; le quattro porte possono essere indipendenti.
    #[arg(long)]
    bind: SocketAddr,
    /// Indirizzo del successore nell’anello ATM1 → ATM2 → ATM3 → ATM4 → ATM1.
    #[arg(long)]
    successor: SocketAddr,
    /// Richiesta FIFO ripetibile, per esempio deposit:100 oppure withdraw:200.
    #[arg(long = "transaction")]
    transactions: Vec<ETransaction>,
    /// Obbligatorio per ATM1, vietato per gli altri tre nodi.
    #[arg(long)]
    initial_token: bool,
    /// Saldo iniziale in unità intere, senza aritmetica in virgola mobile.
    #[arg(long, default_value_t = SPEC_INITIAL_BALANCE)]
    initial_balance: i64,
    /// Attesa iniziale in millisecondi; i tentativi di connessione gestiscono avvii più lenti.
    #[arg(long, default_value_t = 1500)]
    startup_delay_ms: u64,
    /// Intervallo fra tentativi di connessione; una scrittura incerta non viene ritentata.
    #[arg(long, default_value_t = 500, value_parser = clap::value_parser!(u64).range(1..))]
    retry_delay_ms: u64,
    /// Ritardo didattico in millisecondi all’interno della sezione critica attiva.
    /// Un nodo inattivo inoltra subito il token, senza pause deliberate.
    #[arg(long, default_value_t = 0)]
    transaction_delay_ms: u64,
    /// File del saldo, identico per tutti i nodi; ATM1 lo crea senza sovrascriverlo.
    #[arg(long, default_value = "shared.txt")]
    account_file: std::path::PathBuf,
    /// Prima delle quattro porte UDP dei log, uguale per tutti i nodi.
    #[arg(long, default_value_t = 6001, value_parser = clap::value_parser!(u16).range(1024..=65532))]
    log_base_port: u16,
    /// Emette eventi JSONL strutturati nella console.
    #[arg(long)]
    json: bool,
    /// Nuovo file JSONL facoltativo, scritto esclusivamente da questo nodo.
    #[arg(long)]
    audit_file: Option<std::path::PathBuf>,
    /// Mostra in console solo fino a questo hop; --audit-file conserva tutti gli eventi.
    #[arg(long, requires = "audit_file")]
    console_hop_limit: Option<u64>,
}

/// Converte una validazione di dominio in errore di configurazione della CLI.
fn input_error(error: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error)
}

/// Avvia un solo nodo e conserva il permesso fino al completamento di transazione e log.
#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();
    CConfig::validate(
        &cli.id,
        cli.bind,
        cli.successor,
        cli.initial_token,
        cli.initial_balance,
    )
    .map_err(input_error)?;
    let listener = CMessage::bind_listener(cli.bind).await?;
    let mut expected_hop = u64::from(CAtm::index(&cli.id).map_err(input_error)?);
    let udp = BUdpLog::bind(
        CAtm::index(&cli.id).map_err(input_error)? + 1,
        cli.log_base_port,
        cli.json,
        cli.console_hop_limit,
    )?;
    let mut state = CNode::new(cli.id, cli.transactions);
    let mut log = BLog::new(state.id(), cli.json)
        .with_audit_file(cli.audit_file.as_deref())?
        .with_console_hop_limit(cli.console_hop_limit)
        .with_udp(udp);
    log.event(
        "node_ready",
        None,
        None,
        json!({"bind":cli.bind,"successor":cli.successor,"pending":state.pending_len(),"pid":std::process::id(),"account_file":cli.account_file,"log_base_port":cli.log_base_port}),
    )?;

    // ATM1 crea il token localmente una sola volta, dopo il bind riuscito.
    // Non esiste un iniettore asincrono concorrente con la ricezione.
    // I messaggi ordinari sono indirizzati soltanto al successore.
    let mut initial = if cli.initial_token {
        sleep(Duration::from_millis(cli.startup_delay_ms)).await;
        // Il bootstrap detiene già il solo permesso: crea la risorsa prima del primo invio.
        BAccount::initialize(&cli.account_file, cli.initial_balance)?;
        let token = CToken::create_initial(cli.initial_balance);
        log.event("token_created", Some(0), Some(token.balance), json!({}))?;
        Some(token)
    } else {
        None
    };

    // Un canale in ingresso e uno verso il solo successore rimangono aperti.
    // Il permesso resta nel token, non nell’esistenza delle connessioni TCP.
    let mut incoming: Option<BufReader<TcpStream>> = None;
    let mut outgoing: Option<TcpStream> = None;
    loop {
        let mut token = if let Some(token) = initial.take() {
            token
        } else {
            if incoming.is_none() {
                let (stream, _) = listener.accept().await?;
                incoming = Some(BufReader::new(stream));
            }
            match CMessage::read_next_token(incoming.as_mut().expect("Lettore inizializzato")).await
            {
                Ok(Some(token)) => token,
                Ok(None) => {
                    incoming = None;
                    continue;
                }
                Err(error) => {
                    incoming = None;
                    log.event(
                        "message_rejected",
                        None,
                        None,
                        json!({"reason":error.to_string()}),
                    )?;
                    continue;
                }
            }
        };
        if let Err(error) = CToken::validate_route(&token, state.atm(), expected_hop) {
            log.event(
                "token_rejected",
                Some(token.hop),
                Some(token.balance),
                json!({"reason":error}),
            )?;
            continue;
        }
        // Riserviamo l’hop successivo prima di modificare lo stato.
        // L’esaurimento del contatore arresta il nodo senza ricreare il token.
        expected_hop = expected_hop
            .checked_add(4)
            .ok_or_else(|| io::Error::other("sequenza dei passaggi esaurita"))?;
        let hop = token.hop;
        log.event(
            "token_received",
            Some(hop),
            Some(token.balance),
            json!({"last_holder":token.last_holder}),
        )?;
        if let Some(transaction) = state.next_transaction() {
            log.event(
                "transaction_started",
                Some(hop),
                Some(token.balance),
                json!({"transaction":transaction}),
            )?;
            // Il file è la fonte del saldo; il valore nel token è soltanto un controllo di coerenza.
            let stored = BAccount::read(&cli.account_file)?;
            if stored != token.balance {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "saldo del file diverso dall’ultimo valore scritto nell’anello",
                ));
            }
            token.balance = stored;
            if cli.transaction_delay_ms != 0 {
                sleep(Duration::from_millis(cli.transaction_delay_ms)).await;
            }
        }
        // Lettura, validazione, aggiornamento e log avvengono conservando il permesso.
        // Nessun secondo token è elaborato e nessun invio avviene dentro la CS.
        let report = state.process_token(token).map_err(input_error)?;
        if let Some(outcome) = &report.outcome {
            // Completa la scrittura condivisa mentre possiede ancora il token.
            BAccount::write(&cli.account_file, outcome.new_balance)?;
            log.event(
                "transaction_finished",
                Some(hop),
                Some(outcome.new_balance),
                json!({"outcome":outcome}),
            )?;
        }
        log.event(
            "token_forwarding",
            Some(report.token.hop),
            Some(report.token.balance),
            json!({"successor":cli.successor}),
        )?;
        // Un errore I/O dopo la connessione arresta il processo.
        // In caso di consegna incerta fermiamo il progresso per evitare duplicati.
        // La copia locale non autorizza un ulteriore ingresso in sezione critica.
        CMessage::send_token_reusing_connection(
            &mut outgoing,
            cli.successor,
            &report.token,
            Duration::from_millis(cli.retry_delay_ms),
        )
        .await?;
        log.event(
            "token_forwarded",
            Some(report.token.hop),
            Some(report.token.balance),
            json!({"successor":cli.successor}),
        )?;
    }
}
