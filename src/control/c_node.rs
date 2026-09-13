//! Stato privato di un ATM ed elaborazione di una richiesta per visita del token.

use crate::control::{CToken, CTransaction};
use crate::entity::{
    EAtm, ENodeState, EToken, ETokenProcessingReport, ETransaction, ETransactionOutcome,
};
use std::collections::VecDeque;

/// Controllo di un singolo ATM con stato locale [`ENodeState`].
///
/// Gestisce una richiesta per visita; socket, delimitazione dei log e possesso
/// esclusivo del token durante l’intero ciclo sono responsabilità del binario.
#[derive(Clone, Debug)]
pub struct CNode {
    state: ENodeState,
}

impl CNode {
    /// Crea lo stato locale e raccoglie le richieste nell’ordine FIFO ricevuto.
    ///
    /// Non apre socket e non crea token. Il binario valida prima la configurazione;
    /// [`Self::process_token`] rivalida l’identificativo e il dominio delle richieste.
    pub fn new(atm: EAtm, transactions: impl IntoIterator<Item = ETransaction>) -> Self {
        Self {
            state: ENodeState {
                atm,
                pending: transactions.into_iter().collect::<VecDeque<_>>(),
                completed: Vec::new(),
            },
        }
    }

    /// Restituisce l’entità identificativa dell’ATM, senza modificarla.
    pub fn atm(&self) -> &EAtm {
        &self.state.atm
    }

    /// Restituisce il nome logico dell’ATM; non il PID del processo.
    pub fn id(&self) -> &str {
        &self.state.atm.id
    }

    /// Consulta la prima richiesta senza estrarla, per registrare l’ingresso in sezione critica.
    pub fn next_transaction(&self) -> Option<&ETransaction> {
        self.state.pending.front()
    }

    /// Restituisce il numero di richieste in attesa delle prossime visite del token.
    pub fn pending_len(&self) -> usize {
        self.state.pending.len()
    }

    /// Restituisce lo storico locale dei tentativi, inclusi quelli rifiutati.
    pub fn completed(&self) -> &[ETransactionOutcome] {
        &self.state.completed
    }

    /// Espone una vista immutabile dello stato privato del nodo.
    pub const fn state(&self) -> &ENodeState {
        &self.state
    }

    ///
    /// Elabora al massimo una richiesta FIFO e restituisce il token da inoltrare.
    ///
    /// In assenza di richieste aggiorna solo i metadati, senza modificare il saldo
    /// né introdurre ritardi. Un tentativo rifiutato viene comunque estratto dalla
    /// coda e registrato nello storico. Il chiamante gestisce i log e l’inoltro TCP.
    ///
    /// # Precondizione di protocollo
    /// Il binario deve prima chiamare [`CToken::validate_route`] con l’hop atteso;
    /// questa funzione verifica il dominio ma non conserva la sequenza di rete.
    ///
    /// # Errori
    /// Rifiuta token o identificativo invalidi prima di consumare la coda.
    pub fn process_token(&mut self, mut token: EToken) -> Result<ETokenProcessingReport, String> {
        CToken::validate(&token)?;
        crate::CAtm::index(&self.state.atm)?;
        let received_balance = token.balance;
        let outcome = self.state.pending.pop_front().map(|transaction| {
            let (new_balance, outcome) =
                CTransaction::apply(&self.state.atm, token.balance, transaction);
            token.balance = new_balance;
            self.state.completed.push(outcome.clone());
            outcome
        });

        CToken::mark_processed_by(&mut token, &self.state.atm)?;

        Ok(ETokenProcessingReport {
            atm: self.state.atm.clone(),
            received_balance,
            forwarded_balance: token.balance,
            token,
            outcome,
            remaining_pending: self.state.pending.len(),
        })
    }
}
