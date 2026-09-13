//! Risultato dell’elaborazione di una visita del token.

use crate::entity::{EAtm, EToken, ETransactionOutcome};
use serde::{Deserialize, Serialize};

/// Rapporto di una singola visita del token a un ATM.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ETokenProcessingReport {
    /// ATM che ha elaborato la visita.
    pub atm: EAtm,
    /// Saldo letto all’arrivo del token.
    pub received_balance: i64,
    /// Saldo dopo l’eventuale operazione, uguale a quello del token in uscita.
    pub forwarded_balance: i64,
    /// Token da inoltrare esclusivamente al successore.
    pub token: EToken,
    /// Esito del tentativo, oppure `None` se non c’erano richieste.
    pub outcome: Option<ETransactionOutcome>,
    /// Numero di richieste rimaste in coda per le visite successive.
    pub remaining_pending: usize,
}
