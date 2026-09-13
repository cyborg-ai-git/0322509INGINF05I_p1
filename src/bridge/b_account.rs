//! Accesso al file del conto nella sezione critica protetta dal possesso del token.

use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};

/// Lettura e scrittura del conto senza lock di file o coordinatori aggiuntivi.
///
/// Il chiamante deve possedere il solo token durante ogni accesso. Il file è la
/// risorsa protetta, non un meccanismo di sincronizzazione. Ogni processo mantiene
/// memoria privata e apre lo stesso percorso soltanto nella propria sezione critica.
pub struct BAccount;

impl BAccount {
    /// Crea il conto iniziale una sola volta, prima della prima cessione del token.
    ///
    /// # Errori
    /// Rifiuta saldi negativi, percorsi non scrivibili e file già esistenti: un
    /// secondo bootstrap non deve azzerare un conto creato da un'altra esecuzione.
    pub fn initialize(path: &Path, balance: i64) -> io::Result<()> {
        Self::validate(balance)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        writeln!(file, "{balance}")?;
        file.sync_all()
    }

    /// Legge il saldo corrente dal file mentre il chiamante conserva il token.
    ///
    /// # Errori
    /// Rifiuta file mancanti, più di 64 byte, UTF-8 errato, valori negativi e dati
    /// diversi da un unico intero. Non ricostruisce mai il saldo da una copia locale.
    pub fn read(path: &Path) -> io::Result<i64> {
        let mut text = String::new();
        fs::File::open(path)?.take(65).read_to_string(&mut text)?;
        if text.len() > 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file del conto troppo lungo",
            ));
        }
        let balance = text
            .trim()
            .parse::<i64>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "saldo del file non valido"))?;
        Self::validate(balance)?;
        Ok(balance)
    }

    /// Sostituisce il contenuto del conto prima di registrare la fine della transazione.
    ///
    /// Scrive un temporaneo nella stessa directory, sincronizza i dati e lo rinomina.
    /// Il rename evita un saldo parzialmente leggibile; non concede il token e non
    /// sostituisce la mutua esclusione. Non implementa recupero transazionale dai crash.
    ///
    /// # Errori
    /// Propaga errori di scrittura, sincronizzazione e rename. Il chiamante deve
    /// arrestarsi senza inoltrare il token quando l'esito della scrittura è incerto.
    pub fn write(path: &Path, balance: i64) -> io::Result<()> {
        Self::validate(balance)?;
        let name = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "percorso del conto non valido")
        })?;
        let temporary = path.with_file_name(format!(
            ".{}.{}.tmp",
            name.to_string_lossy(),
            std::process::id()
        ));
        // create_new impedisce di sovrascrivere un temporaneo appartenente a un altro avvio.
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| {
            writeln!(file, "{balance}")?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn validate(balance: i64) -> io::Result<()> {
        if balance < 0 {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "il saldo non può essere negativo",
            ))
        } else {
            Ok(())
        }
    }
}
