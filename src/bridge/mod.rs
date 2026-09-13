//! Adattatori per TCP e registrazione degli eventi.
//!
//! I tipi con prefisso `B` isolano le operazioni di ingresso/uscita dalle regole
//! del dominio. I file di audit non partecipano alla mutua esclusione distribuita.

pub mod b_tcp;

pub use b_tcp::BTcp;

pub mod b_log;
pub use b_log::BLog;

/// Saldo condiviso su file, protetto dal token.
pub mod b_account;
pub use b_account::BAccount;
/// Diffusione dei log fra le console tramite UDP.
pub mod b_udp_log;
pub use b_udp_log::BUdpLog;
