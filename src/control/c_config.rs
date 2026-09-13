//! Validazione degli argomenti prima di aprire il listener del nodo.

use crate::{CAtm, EAtm};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Controllo della topologia fissa tramite il proprio indirizzo e quello del successore.
pub struct CConfig;
impl CConfig {
    /// Verifica topologia, bootstrap e saldo prima dell’apertura delle porte.
    ///
    /// Porta locale e porta del successore sono parametri indipendenti e non devono
    /// essere consecutive. Il chiamante collega i quattro nodi nell’ordine logico;
    /// ogni processo conosce solo il proprio successore. Solo ATM1 crea il token.
    ///
    /// # Errori
    /// Rifiuta identificativi invalidi, indirizzi diversi da `127.0.0.1`, porte
    /// nulle o coincidenti, bootstrap sul nodo sbagliato e saldo negativo.
    pub fn validate(
        atm: &EAtm,
        bind: SocketAddr,
        successor: SocketAddr,
        initial_token: bool,
        initial_balance: i64,
    ) -> Result<(), String> {
        let position = CAtm::index(atm)?;
        let localhost = IpAddr::V4(Ipv4Addr::LOCALHOST);
        if bind.ip() != localhost || successor.ip() != localhost {
            return Err("entrambi gli indirizzi devono usare 127.0.0.1".into());
        }
        if bind.port() == 0 || successor.port() == 0 || bind == successor {
            return Err(
                "porta locale e porta del successore devono essere non nulle e distinte".into(),
            );
        }
        if initial_token != (position == 0) {
            return Err(
                "--initial-token è obbligatorio per ATM1 e vietato per ATM2/ATM3/ATM4".into(),
            );
        }
        if initial_balance < 0 {
            return Err("il saldo iniziale deve essere non negativo".into());
        }
        Ok(())
    }
}
