//! Entità dati della simulazione bancaria.
//!
//! I moduli `e_*` contengono strutture con prefisso `E`; i moduli `enum_*`
//! contengono enumerazioni con prefisso `Enum`. Le regole di validazione sono nei
//! controlli. I campi pubblici rendono necessaria la validazione ai confini del dominio.

pub mod e_atm;
pub mod e_node_state;
pub mod e_token;
pub mod e_token_processing_report;
pub mod e_transaction;
pub mod e_transaction_outcome;
pub mod enum_transaction_kind;
pub mod enum_transaction_status;

pub use e_atm::EAtm;
pub use e_node_state::ENodeState;
pub use e_token::EToken;
pub use e_token_processing_report::ETokenProcessingReport;
pub use e_transaction::ETransaction;
pub use e_transaction_outcome::ETransactionOutcome;
pub use enum_transaction_kind::EnumTransactionKind;
pub use enum_transaction_status::EnumTransactionStatus;

/// Numero di nodi richiesto dalla traccia: esattamente quattro.
pub const REQUIRED_NODE_COUNT: usize = 4;
