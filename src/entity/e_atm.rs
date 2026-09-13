//! Identificativo logico di un ATM.

use serde::{Deserialize, Serialize};

/// Identificativo logico di un processo ATM.
///
/// Il campo `id` contiene uno dei valori canonici `ATM1`, `ATM2`, `ATM3`, `ATM4`.
/// La validazione è implementata da [`crate::CAtm`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EAtm {
    /// Nome canonico dell’ATM; non coincide con il PID del sistema operativo.
    pub id: String,
}
