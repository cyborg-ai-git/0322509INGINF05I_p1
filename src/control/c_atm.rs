//! Identificazione canonica dei quattro ATM e navigazione nell’anello fisso.

use crate::entity::EAtm;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// Validazione degli identificativi e calcolo del successore nell’anello.
pub struct CAtm;

impl CAtm {
    /// Numero di processi previsto dalla topologia dell’elaborato.
    pub const NODE_COUNT: u16 = 4;

    /// Restituisce la posizione nell’anello, da 0 per ATM1 a 3 per ATM4.
    ///
    /// # Errori
    /// Rifiuta un campo pubblico `id` diverso dai quattro nomi canonici.
    pub fn index(atm: &EAtm) -> Result<u16, String> {
        let number = Self::parse_number(atm.id.strip_prefix("ATM").unwrap_or(""))?;
        if atm.id != format!("ATM{number}") {
            return Err("l’identificativo interno deve avere forma canonica ATM<n>".into());
        }
        Ok(number - 1)
    }

    /// Costruisce un identificativo canonico, eliminando gli spazi esterni.
    ///
    /// Accetta anche gli alias `1`…`4` e non distingue maiuscole e minuscole.
    ///
    /// # Errori
    /// Restituisce una descrizione se il nome non identifica uno dei quattro ATM.
    pub fn build(id: impl Into<String>) -> Result<EAtm, String> {
        let id = Self::normalize_id(&id.into())?;
        Ok(EAtm { id })
    }

    /// Restituisce il successore nell’ordine ATM1 → ATM2 → ATM3 → ATM4 → ATM1.
    ///
    /// # Errori
    /// Rifiuta un identificativo non canonico, anche se costruito direttamente.
    pub fn successor(atm: &EAtm) -> Result<EAtm, String> {
        let index = Self::index(atm)?;
        Self::build(((index + 1) % Self::NODE_COUNT + 1).to_string())
    }

    /// Normalizza soltanto gli alias ammessi, senza inventare nuovi nodi.
    fn normalize_id(value: &str) -> Result<String, String> {
        let normalized = value.trim().to_ascii_uppercase();
        let number = Self::parse_number(normalized.strip_prefix("ATM").unwrap_or(&normalized))?;
        Ok(format!("ATM{number}"))
    }

    /// Converte il parametro numerico e ne verifica l’appartenenza all’anello.
    fn parse_number(value: &str) -> Result<u16, String> {
        value
            .parse::<u16>()
            .ok()
            .filter(|number| (1..=Self::NODE_COUNT).contains(number) && value == number.to_string())
            .ok_or_else(|| {
                format!(
                    "identificativo '{value}' non valido: atteso un numero da 1 a {}",
                    Self::NODE_COUNT
                )
            })
    }
}

impl Display for EAtm {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.id)
    }
}

impl FromStr for EAtm {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        CAtm::build(value)
    }
}
