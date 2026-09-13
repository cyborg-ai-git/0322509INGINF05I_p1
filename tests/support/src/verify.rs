//! Verifica causale dei log privati dei quattro ATM, senza intervenire sul token.
use crate::Result;
use anyhow::{Context, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

fn number(event: &Value, key: &str) -> Result<u64> {
    event[key]
        .as_u64()
        .with_context(|| format!("Campo numerico non valido: {key}"))
}

fn timestamp(event: &Value, key: &str) -> Result<u128> {
    Ok(event[key].as_str().context("Timestamp mancante")?.parse()?)
}

/// Legge i record completi e conta soltanto le ultime righe senza newline.
pub fn read_events(directory: &Path) -> Result<(Vec<Value>, usize)> {
    let mut events = Vec::new();
    let mut truncated = 0;
    for node in 1..=4 {
        let raw = fs::read(directory.join(format!("atm{node}.jsonl")))?;
        for line in raw.split_inclusive(|b| *b == b'\n') {
            if !line.ends_with(b"\n") {
                truncated += 1;
                continue;
            }
            let event: Value = serde_json::from_slice(line).context("Record JSONL non valido")?;
            ensure!(
                event["atm"] == format!("ATM{node}"),
                "Evento nel file di un altro nodo"
            );
            events.push(event);
        }
    }
    Ok((events, truncated))
}

/// Controlla un numero intero di giri, le operazioni e gli intervalli di esclusione.
/// I rifiuti da EOF sono ammessi soltanto dopo un arresto esterno documentato.
pub fn verify(directory: &Path, hops: u64, specification: bool) -> Result<Value> {
    ensure!(
        hops >= 4 && hops.is_multiple_of(4),
        "Verificare un numero intero di giri"
    );
    let (events, truncated) = read_events(directory)?;
    let mut ready = Vec::new();
    let mut created = Vec::new();
    let mut rejected = Vec::new();
    let mut tables: BTreeMap<&str, BTreeMap<u64, &Value>> = BTreeMap::new();
    for kind in [
        "token_received",
        "token_forwarding",
        "transaction_started",
        "transaction_finished",
    ] {
        tables.insert(kind, BTreeMap::new());
    }
    for node in 1..=4 {
        let local: Vec<_> = events
            .iter()
            .filter(|e| e["atm"] == format!("ATM{node}"))
            .collect();
        ensure!(!local.is_empty(), "Log del nodo assente");
        let pid = number(local[0], "pid")?;
        for (index, event) in local.iter().enumerate() {
            ensure!(
                number(event, "seq")? == index as u64 + 1,
                "Lacuna nella sequenza locale"
            );
            ensure!(
                number(event, "pid")? == pid,
                "Un log contiene processi diversi"
            );
        }
    }
    for event in &events {
        let kind = event["event"].as_str().context("Tipo di evento assente")?;
        ensure!(kind != "token_rejected", "Token rifiutato: {event}");
        match kind {
            "node_ready" => ready.push(event),
            "token_created" => created.push(event),
            "message_rejected" => rejected.push(event),
            _ => {}
        }
        if let Some(table) = tables.get_mut(kind) {
            ensure!(
                table.insert(number(event, "hop")?, event).is_none(),
                "Evento duplicato: {kind}"
            );
        }
    }
    let pids: HashSet<_> = ready
        .iter()
        .map(|e| number(e, "pid"))
        .collect::<Result<_>>()?;
    ensure!(
        ready.len() == 4 && pids.len() == 4,
        "Occorrono quattro PID distinti"
    );
    ensure!(
        created.len() == 1 && created[0]["atm"] == "ATM1",
        "Occorre un solo bootstrap ATM1"
    );
    let get = |kind: &str, hop: u64| -> Result<&Value> {
        tables
            .get(kind)
            .and_then(|t| t.get(&hop))
            .copied()
            .with_context(|| format!("Evento mancante: {kind} hop {hop}"))
    };
    ensure!(
        created[0]["balance"] == get("token_received", 0)?["balance"],
        "Saldo di bootstrap incoerente"
    );
    let mut transactions = Vec::new();
    let mut previous_end = 0;
    for hop in 0..hops {
        let receipt = get("token_received", hop)?;
        let outgoing = get("token_forwarding", hop + 1)?;
        ensure!(
            receipt["atm"] == format!("ATM{}", hop % 4 + 1),
            "Ordine dell'anello errato"
        );
        ensure!(
            outgoing["atm"] == receipt["atm"],
            "Inoltro attribuito a un altro ATM"
        );
        ensure!(
            number(receipt, "seq")? < number(outgoing, "seq")?,
            "Inoltro prima della ricezione"
        );
        ensure!(
            timestamp(receipt, "unix_ns")? <= timestamp(outgoing, "unix_ns")?,
            "Timestamp locali fuori ordine"
        );
        if hop > 0 {
            ensure!(
                get("token_forwarding", hop)?["balance"] == receipt["balance"],
                "Discontinuità del saldo TCP"
            );
            ensure!(
                timestamp(get("token_forwarding", hop)?, "unix_ns")?
                    <= timestamp(receipt, "unix_ns")?,
                "Ricezione antecedente all’invio"
            );
        }
        if let Some(start) = tables["transaction_started"].get(&hop) {
            let end = get("transaction_finished", hop)?;
            ensure!(
                start["atm"] == end["atm"] && end["atm"] == receipt["atm"],
                "Transazione di un altro ATM"
            );
            ensure!(
                number(receipt, "seq")? < number(start, "seq")?
                    && number(start, "seq")? < number(end, "seq")?
                    && number(end, "seq")? < number(outgoing, "seq")?,
                "Sezione critica fuori ordine"
            );
            let begin = timestamp(start, "unix_ns")?;
            let finish = timestamp(end, "unix_ns")?;
            ensure!(
                timestamp(receipt, "unix_ns")? <= begin
                    && finish <= timestamp(outgoing, "unix_ns")?,
                "Timestamp della sezione critica fuori dalla visita"
            );
            ensure!(
                previous_end <= begin && begin <= finish,
                "Sezioni critiche sovrapposte"
            );
            previous_end = finish;
            let outcome = &end["detail"]["outcome"];
            let before = number(outcome, "previous_balance")?;
            ensure!(
                before == number(receipt, "balance")? && before == number(start, "balance")?,
                "Lettura del saldo incoerente"
            );
            let tx = &outcome["transaction"];
            let amount = number(tx, "amount")?;
            ensure!(
                amount > 0 && before <= i64::MAX as u64,
                "Importo o saldo non valido"
            );
            let candidate = match tx["kind"].as_str() {
                Some("Deposit") => before.checked_add(amount).filter(|v| *v <= i64::MAX as u64),
                Some("Withdrawal") => before.checked_sub(amount),
                _ => anyhow::bail!("Tipo di transazione non valido"),
            };
            ensure!(
                (outcome["status"] == "Completed") == candidate.is_some(),
                "Accettazione o rifiuto errato"
            );
            if candidate.is_none() {
                ensure!(
                    outcome["status"].get("Rejected").is_some(),
                    "Motivo del rifiuto mancante"
                );
            }
            let expected = candidate.unwrap_or(before);
            ensure!(
                number(outcome, "new_balance")? == expected
                    && number(end, "balance")? == expected
                    && number(outgoing, "balance")? == expected,
                "Saldo finale della transazione errato"
            );
            transactions.push(json!({"hop":hop,"atm":end["atm"],"before":before,"after":expected,"transaction":tx,"status":outcome["status"]}));
        } else {
            ensure!(
                !tables["transaction_finished"].contains_key(&hop),
                "Fine senza inizio della transazione"
            );
            ensure!(
                outgoing["balance"] == receipt["balance"],
                "Nodo inattivo modifica il saldo"
            );
        }
    }
    if specification {
        let actual: Vec<_> = transactions
            .iter()
            .map(|t| json!([t["atm"], t["before"], t["after"]]))
            .collect();
        ensure!(
            actual
                == vec![
                    json!(["ATM2", 1000, 800]),
                    json!(["ATM3", 800, 900]),
                    json!(["ATM4", 900, 400])
                ],
            "Scenario diverso dalla traccia"
        );
    }
    if !rejected.is_empty() {
        let shutdown: Value = serde_json::from_slice(&fs::read(directory.join("shutdown.json"))?)?;
        let stopped: HashSet<_> = shutdown["pids"]
            .as_array()
            .context("PID di arresto assenti")?
            .iter()
            .map(|p| p.as_u64().context("PID non valido"))
            .collect::<Result<_>>()?;
        ensure!(stopped == pids, "Arresto di processi diversi");
        for event in &rejected {
            let last = (1..=hops)
                .filter_map(|h| tables["token_forwarding"].get(&h))
                .rfind(|e| e["atm"] == event["atm"])
                .context("Confine del prefisso assente")?;
            ensure!(
                number(event, "seq")? > number(last, "seq")?,
                "Rifiuto nel prefisso verificato"
            );
            ensure!(
                timestamp(event, "unix_ns")? >= timestamp(&shutdown, "requested_unix_ns")?,
                "Rifiuto prima dell'arresto"
            );
            ensure!(
                event["detail"]["reason"] == "messaggio incompleto: manca la newline finale",
                "Rifiuto diverso da EOF di arresto"
            );
        }
    }
    // Nello scenario finito il conto deve conservare il risultato delle tre operazioni.
    if specification {
        let stored: i64 = fs::read_to_string(directory.join("shared.txt"))?
            .trim()
            .parse()?;
        ensure!(
            stored == 400,
            "Il file del conto non contiene il saldo finale 400"
        );
    }
    Ok(
        json!({"verified":true,"processes":4,"checked_hops":hops,"checked_rounds":hops/4,
        "initial_balance":get("token_received",0)?["balance"],"final_balance":get("token_forwarding",hops)?["balance"],
        "transactions":transactions,"complete_records":events.len(),"truncated_tail_lines":truncated,
        "shutdown_frame_rejections":rejected.len(),"scope":"Prefisso causale finito; nessuna prova universale per tutti gli scenari di guasto"}),
    )
}
