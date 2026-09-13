//! Permesso, copia diagnostica del saldo e metadati trasportati dal token.

use crate::entity::EAtm;
use serde::{Deserialize, Serialize};

/// Permesso che circola nell’anello logico con una copia dell’ultimo saldo scritto.
///
/// Il saldo autorevole è letto dal file del conto dentro la sezione critica.
/// Il campo balance permette di controllare coerenza e log del passaggio, ma
/// non sostituisce la lettura del file e non autorizza da solo una transazione.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EToken {
    /// Copia dell’ultimo saldo scritto sul file, in unità intere non negative.
    pub balance: i64,
    /// Numero di elaborazioni del token; cresce di uno a ogni visita, anche inattiva.
    pub hop: u64,
    /// Ultimo ATM che ha elaborato il token; `None` solo al bootstrap.
    pub last_holder: Option<EAtm>,
}
