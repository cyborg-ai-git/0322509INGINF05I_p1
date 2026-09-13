//! Regole del dominio: identità, configurazione, FIFO, transazioni, token e messaggi.
//!
//! I controlli validano dati privati al nodo; non introducono memoria condivisa.

pub mod c_atm;
pub mod c_message;
pub mod c_node;
pub mod c_token;
pub mod c_transaction;

pub use c_atm::CAtm;
pub use c_message::CMessage;
pub use c_node::CNode;
pub use c_token::CToken;
pub use c_transaction::CTransaction;

pub mod c_config;
pub use c_config::CConfig;
