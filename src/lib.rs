//! Libreria della simulazione bancaria distribuita con mutua esclusione a Token Ring.
//!
//! Il binario avvia un solo ATM per processo. Il saldo autorevole è nel file del conto;
//! ogni nodo possiede esclusivamente la propria coda e il proprio storico locale.
//! I controlli di dominio sono separati dal trasporto TCP e dall'osservazione dei log.
//!
//! # Esempio: la transazione richiesta ad ATM2
//!
//! ```
//! use app_atm::{CAtm, CTransaction};
//! let atm = CAtm::build("ATM2")?;
//! let richiesta = CTransaction::withdrawal(200)?;
//! let (saldo, esito) = CTransaction::apply(&atm, 1000, richiesta);
//! assert_eq!(saldo, 800);
//! assert!(CTransaction::is_completed(&esito));
//! # Ok::<(), String>(())
//! ```
//!
//! # Garanzie e limiti
//!
//! Il protocollo richiede quattro nodi cooperanti e un solo bootstrap, eseguito da
//! ATM1. I tipi dati pubblici consentono copie per test e registrazioni: l'unicità
//! del permesso deriva dal protocollo del binario, non da un tipo lineare Rust.
//! Il file conserva il saldo dopo l’arresto, ma non implementa recupero automatico dai guasti.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod bridge;
pub mod control;
pub mod entity;

pub use bridge::{BAccount, BLog, BTcp, BUdpLog};
pub use control::{CAtm, CConfig, CMessage, CNode, CToken, CTransaction};
pub use entity::{
    EAtm, ENodeState, EToken, ETokenProcessingReport, ETransaction, ETransactionOutcome,
    EnumTransactionKind, EnumTransactionStatus, REQUIRED_NODE_COUNT,
};

/// Saldo iniziale di 1000 unità richiesto dalla traccia del Progetto 1.
pub const SPEC_INITIAL_BALANCE: i64 = 1000;
