//! Processo di prova: associa ogni riga ricevuta al nome della propria console.
//! Viene compilato dal test delle risorse e non fa parte dell'eseguibile bancario.
use std::io::{self, BufRead, Write};

/// Segnala la disponibilità e risponde fino alla chiusura del proprio ingresso.
fn main() -> io::Result<()> {
    let name = std::env::args()
        .nth(1)
        .expect("Nome della console di prova");
    println!("PRONTO {name}");
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        println!("RISPOSTA {name} {}", line?);
        io::stdout().flush()?;
    }
    Ok(())
}
