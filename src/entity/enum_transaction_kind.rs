//! Operazioni bancarie ammesse.

use serde::{Deserialize, Serialize};

/// Operazione eseguita in una singola sezione critica.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EnumTransactionKind {
    /// Deposito: aggiunge l’importo al saldo con controllo dell’overflow.
    Deposit,
    /// Prelievo: sottrae l’importo soltanto se il saldo è sufficiente.
    Withdrawal,
}
