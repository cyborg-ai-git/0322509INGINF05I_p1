//! Coda e storico privati del nodo.

use crate::entity::{EAtm, ETransaction, ETransactionOutcome};
use std::collections::VecDeque;

/// Stato privato posseduto da un singolo processo ATM.
///
/// Questa entità contiene solo dati; le regole di elaborazione sono in
/// [`crate::CNode`].
#[derive(Clone, Debug)]
pub struct ENodeState {
    /// Identificativo logico di questo ATM.
    pub atm: EAtm,
    /// Coda FIFO locale delle richieste ancora da tentare.
    pub pending: VecDeque<ETransaction>,
    /// Storico locale dei tentativi completati o rifiutati.
    pub completed: Vec<ETransactionOutcome>,
}
