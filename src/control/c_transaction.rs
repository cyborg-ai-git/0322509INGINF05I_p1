//! Validazione, conversione testuale e aritmetica controllata delle transazioni.

use crate::entity::{
    EAtm, ETransaction, ETransactionOutcome, EnumTransactionKind, EnumTransactionStatus,
};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// Regole per validare, interpretare e applicare le operazioni bancarie.
pub struct CTransaction;

impl CTransaction {
    /// Costruisce una richiesta di deposito.
    ///
    /// # Errori
    /// L’importo deve essere strettamente positivo, come in [`Self::build`].
    pub fn deposit(amount: i64) -> Result<ETransaction, String> {
        Self::build(EnumTransactionKind::Deposit, amount)
    }

    /// Costruisce una richiesta di prelievo.
    ///
    /// # Errori
    /// L’importo deve essere strettamente positivo. La disponibilità del saldo viene
    /// verificata solo all’esecuzione, quando il nodo possiede il token.
    pub fn withdrawal(amount: i64) -> Result<ETransaction, String> {
        Self::build(EnumTransactionKind::Withdrawal, amount)
    }

    /// Costruisce una richiesta per il tipo e l’importo indicati.
    ///
    /// # Errori
    /// Rifiuta importi nulli o negativi. Il saldo non è necessario in questa fase.
    pub fn build(kind: EnumTransactionKind, amount: i64) -> Result<ETransaction, String> {
        if amount <= 0 {
            return Err(format!("l’importo deve essere positivo, ricevuto {amount}"));
        }

        Ok(ETransaction { kind, amount })
    }

    ///
    /// Calcola il nuovo saldo e restituisce la registrazione del tentativo.
    ///
    /// Rivalida importo e saldo anche per entità costruite senza passare dal parser.
    /// Usa aritmetica intera controllata. Fondi insufficienti, overflow o valori di
    /// dominio invalidi producono `Rejected` mantenendo il saldo ricevuto invariato.
    /// La funzione non scrive log né acquisisce lock: il chiamante deve possedere il
    /// token per tutta la lettura, applicazione, scrittura e registrazione dell’esito.
    pub fn apply(
        atm: &EAtm,
        balance: i64,
        transaction: ETransaction,
    ) -> (i64, ETransactionOutcome) {
        // I campi pubblici possono essere costruiti senza passare dalla CLI.
        // Rivalidiamo i valori prima di qualsiasi operazione aritmetica.
        let invalid = if balance < 0 {
            Some("il saldo deve essere non negativo")
        } else if transaction.amount <= 0 {
            Some("l’importo deve essere positivo")
        } else {
            None
        };
        let (new_balance, status) = if let Some(reason) = invalid {
            (
                balance,
                EnumTransactionStatus::Rejected {
                    reason: reason.into(),
                },
            )
        } else {
            match transaction.kind {
                EnumTransactionKind::Deposit => match balance.checked_add(transaction.amount) {
                    Some(value) => (value, EnumTransactionStatus::Completed),
                    None => (
                        balance,
                        EnumTransactionStatus::Rejected {
                            reason: "il deposito causerebbe overflow del saldo".to_string(),
                        },
                    ),
                },
                EnumTransactionKind::Withdrawal if transaction.amount <= balance => (
                    balance - transaction.amount,
                    EnumTransactionStatus::Completed,
                ),
                EnumTransactionKind::Withdrawal => (
                    balance,
                    EnumTransactionStatus::Rejected {
                        reason: format!(
                            "fondi insufficienti: saldo {balance}, importo richiesto {}",
                            transaction.amount
                        ),
                    },
                ),
            }
        };

        let outcome = ETransactionOutcome {
            atm: atm.clone(),
            transaction,
            previous_balance: balance,
            new_balance,
            status,
        };

        (new_balance, outcome)
    }

    /// Restituisce `true` se il tentativo ha modificato il saldo con esito positivo.
    pub fn is_completed(outcome: &ETransactionOutcome) -> bool {
        outcome.status == EnumTransactionStatus::Completed
    }
}

impl Display for EnumTransactionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Deposit => "deposit",
            Self::Withdrawal => "withdrawal",
        })
    }
}

impl Display for ETransaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.kind, self.amount)
    }
}

impl FromStr for ETransaction {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        let (raw_kind, raw_amount) = trimmed
            .split_once(':')
            .or_else(|| trimmed.split_once('='))
            .ok_or_else(|| {
                format!(
                    "transazione '{trimmed}' non valida: atteso deposit:100 oppure withdraw:100"
                )
            })?;

        let amount = raw_amount
            .trim()
            .parse::<i64>()
            .map_err(|error| format!("importo '{raw_amount}' non valido: {error}"))?;

        match raw_kind.trim().to_ascii_lowercase().as_str() {
            "deposit" | "dep" => CTransaction::deposit(amount),
            "withdraw" | "withdrawal" | "wd" => CTransaction::withdrawal(amount),
            other => Err(format!(
                "operazione '{other}' non valida: atteso deposit oppure withdraw"
            )),
        }
    }
}
