//! Richiesta di deposito o prelievo.

use crate::entity::EnumTransactionKind;
use serde::{Deserialize, Serialize};

/// Richiesta di operazione bancaria.
///
/// I campi contengono solo dati; costruzione e validazione sono affidate a
/// [`crate::CTransaction`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ETransaction {
    /// Tipo di operazione richiesta.
    pub kind: EnumTransactionKind,
    /// Importo intero strettamente positivo; viene rivalidato prima dell’aritmetica.
    pub amount: i64,
}
