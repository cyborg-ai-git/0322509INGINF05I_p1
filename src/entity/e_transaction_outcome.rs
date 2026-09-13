//! Esito completo di un tentativo di transazione.

use crate::entity::{EAtm, ETransaction, EnumTransactionStatus};
use serde::{Deserialize, Serialize};

/// Registrazione completa di un tentativo di transazione.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ETransactionOutcome {
    /// ATM che possedeva il token ed ha eseguito la sezione critica.
    pub atm: EAtm,
    /// Richiesta effettivamente tentata.
    pub transaction: ETransaction,
    /// Saldo precedente alla validazione e all’aggiornamento.
    pub previous_balance: i64,
    /// Saldo risultante; rimane invariato in caso di rifiuto.
    pub new_balance: i64,
    /// Esito del tentativo, con motivazione in caso di rifiuto.
    pub status: EnumTransactionStatus,
}
