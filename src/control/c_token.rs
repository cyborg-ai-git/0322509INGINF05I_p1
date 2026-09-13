//! Validazione del saldo e dei metadati di percorso del token.

use crate::CAtm;
use crate::entity::{EAtm, EToken};

/// Creazione del token e gestione controllata dei metadati di percorso.
pub struct CToken;
impl CToken {
    /// Costruisce il token con hop zero e senza precedente possessore.
    ///
    /// Nel binario è chiamata una sola volta, da ATM1 dopo bind e validazione della
    /// configurazione. Questa funzione dati non convalida il saldo e non impone da
    /// sola l’unicità: prima dell’uso occorre [`Self::validate`].
    pub const fn create_initial(initial_balance: i64) -> EToken {
        EToken {
            balance: initial_balance,
            hop: 0,
            last_holder: None,
        }
    }

    /// Valida i campi di dominio di un token, anche se deserializzato o costruito a mano.
    ///
    /// # Errori
    /// Rifiuta saldo negativo, ultimo possessore invalido e contatore senza spazio
    /// per un ulteriore incremento. Non verifica il prossimo destinatario.
    pub fn validate(token: &EToken) -> Result<(), String> {
        if token.balance < 0 {
            return Err("il saldo del token deve essere non negativo".into());
        }
        if let Some(atm) = &token.last_holder {
            CAtm::index(atm)?;
        }
        token
            .hop
            .checked_add(1)
            .ok_or("contatore dei passaggi del token esaurito")?;
        Ok(())
    }

    /// Verifica la visita attesa dal nodo prima di entrare in sezione critica.
    ///
    /// Le prime visite hanno hop 0, 1, 2 e 3; per ogni nodo le successive avanzano
    /// di quattro. L’ultimo possessore deve essere il predecessore logico, tranne
    /// al bootstrap, dove è assente. Questi metadati non autenticano il mittente.
    ///
    /// # Errori
    /// Propaga gli errori di dominio e rifiuta hop, destinatario o predecessore
    /// incompatibili con la visita attesa. Non modifica token o stato locale.
    pub fn validate_route(token: &EToken, atm: &EAtm, expected_hop: u64) -> Result<(), String> {
        Self::validate(token)?;
        let position = u64::from(CAtm::index(atm)?);
        if token.hop != expected_hop || token.hop % 4 != position {
            return Err(format!(
                "hop del token inatteso {}, atteso {expected_hop}",
                token.hop
            ));
        }
        if token.hop == 0 {
            if token.last_holder.is_some() {
                return Err("il token iniziale non deve avere un precedente possessore".into());
            }
        } else {
            let previous = (position + 3) % 4;
            if token
                .last_holder
                .as_ref()
                .map(CAtm::index)
                .transpose()?
                .map(u64::from)
                != Some(previous)
            {
                return Err(
                    "il precedente possessore non corrisponde al percorso dell’anello".into(),
                );
            }
        }
        Ok(())
    }

    /// Incrementa hop e registra l’ATM che ha elaborato la visita.
    ///
    /// # Errori
    /// Valida token e ATM prima di modificarli; rifiuta valori invalidi e overflow.
    /// In caso di errore il token resta invariato, anche nelle build ottimizzate.
    pub fn mark_processed_by(token: &mut EToken, atm: &EAtm) -> Result<(), String> {
        Self::validate(token)?;
        CAtm::index(atm)?;
        token.hop = token
            .hop
            .checked_add(1)
            .ok_or("contatore dei passaggi del token esaurito")?;
        token.last_holder = Some(atm.clone());
        Ok(())
    }
}
