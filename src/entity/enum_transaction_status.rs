//! Esiti possibili di una transazione.

use serde::{Deserialize, Serialize};

/// Esito di una richiesta tentata all’interno della sezione critica.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EnumTransactionStatus {
    /// Operazione completata con aggiornamento del saldo.
    Completed,
    /// Operazione rifiutata, per esempio per fondi insufficienti; saldo invariato.
    Rejected {
        /// Motivazione leggibile del rifiuto.
        reason: String,
    },
}
