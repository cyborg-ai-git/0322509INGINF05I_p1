//! Risorse di una singola console: PTY, figlio Rust, lettore e buffer privati.
use crate::{Result, random_id};
use anyhow::{Context, ensure};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const MAX_BUFFER: usize = 2 * 1024 * 1024;

#[derive(Default)]
struct Output {
    text: String,
    base: usize,
    eof: bool,
    error: Option<String>,
}

/// Una sessione possiede esclusivamente le risorse del proprio processo.
pub struct Session {
    name: String,
    id: String,
    pid: u32,
    tty: String,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    output: Arc<Mutex<Output>>,
}

impl Session {
    /// Avvia il comando senza shell e apre un nuovo log, senza sovrascriverlo.
    pub fn spawn(name: &str, command: CommandBuilder, log: &Path) -> Result<Self> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(log)?;
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 90,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let id = random_id()?;
        #[cfg(unix)]
        let tty = pair
            .master
            .tty_name()
            .context("Nome PTY non disponibile")?
            .display()
            .to_string();
        #[cfg(not(unix))]
        let tty = format!("ConPTY-{id}");
        let mut child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let Some(pid) = child.process_id() else {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("PID del terminale non disponibile");
        };
        let output = Arc::new(Mutex::new(Output::default()));
        let captured = output.clone();
        thread::spawn(move || {
            let mut bytes = [0u8; 8192];
            let mut pending = Vec::new();
            let result: Result<()> = (|| {
                loop {
                    let count = match reader.read(&mut bytes) {
                        Ok(0) => break,
                        Ok(n) => n,
                        // Linux segnala EIO quando scompare l'ultimo slave PTY.
                        Err(e) if cfg!(unix) && e.raw_os_error() == Some(5) => break,
                        Err(e) => return Err(e.into()),
                    };
                    file.write_all(&bytes[..count])?;
                    file.flush()?;
                    pending.extend_from_slice(&bytes[..count]);
                    append(&captured, &decode(&mut pending, false));
                }
                append(&captured, &decode(&mut pending, true));
                Ok(())
            })();
            let mut state = captured.lock().unwrap_or_else(|e| e.into_inner());
            state.error = result.err().map(|e| e.to_string());
            state.eof = true;
        });
        Ok(Self {
            name: name.into(),
            id,
            pid,
            tty,
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            output,
        })
    }

    /// Identità pubblica del solo terminale, senza stato del conto.
    pub fn identity(&self) -> Value {
        json!({"name":self.name,"atm":self.name,"session":self.id,"pid":self.pid,"pty":self.tty})
    }

    /// Identificativo della sessione usato per instradare input e output.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// PID del processo collegato al PTY.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Controlla e raccoglie l'eventuale terminazione senza attese bloccanti.
    pub fn running(&self) -> Result<bool> {
        Ok(self
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .try_wait()?
            .is_none())
    }

    /// Legge un segmento UTF-8 tramite offset assoluto; non consuma il flusso.
    pub fn snapshot(&self, offset: usize) -> Result<Value> {
        let running = self.running()?;
        let state = self.output.lock().unwrap_or_else(|e| e.into_inner());
        let start = offset.saturating_sub(state.base);
        ensure!(
            start <= state.text.len() && state.text.is_char_boundary(start),
            "Offset della console non valido"
        );
        let mut end = (start + 65536).min(state.text.len());
        while !state.text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(
            json!({"session":self.id,"name":self.name,"pid":self.pid,"tty":self.tty,
            "output":&state.text[start..end],"offset":state.base+end,"reset":offset<state.base,
            "running":running,"eof":state.eof && end==state.text.len(),"error":state.error}),
        )
    }

    /// Scrive solo nel PTY destinatario; rifiuta dati troppo lunghi o processi chiusi.
    pub fn input(&self, data: &str) -> Result<()> {
        ensure!(data.len() <= 8192, "Input troppo lungo");
        ensure!(self.running()?, "Il processo è già terminato");
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        writer.write_all(data.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    /// Modifica esclusivamente le dimensioni del terminale selezionato.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        ensure!(
            (5..=100).contains(&rows) && (20..=300).contains(&cols),
            "Dimensioni non valide"
        );
        self.master
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
    }

    /// Legge le dimensioni effettive dal sistema operativo per verificarne l'isolamento.
    pub fn size(&self) -> Result<(u16, u16)> {
        let size = self
            .master
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_size()?;
        Ok((size.rows, size.cols))
    }

    /// Arresta il solo figlio e attende la fine del lettore, conservando l'output.
    pub fn stop(&self) -> Result<()> {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        child.wait()?;
        drop(child);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !self.output.lock().unwrap_or_else(|e| e.into_inner()).eof {
            ensure!(
                Instant::now() < deadline,
                "Il lettore PTY non ha completato l'arresto"
            );
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn append(output: &Mutex<Output>, text: &str) {
    let mut state = output.lock().unwrap_or_else(|e| e.into_inner());
    state.text.push_str(text);
    let mut excess = state.text.len().saturating_sub(MAX_BUFFER);
    while !state.text.is_char_boundary(excess) {
        excess += 1;
    }
    state.text.drain(..excess);
    state.base += excess;
}

// Conserva i caratteri multibyte incompleti fra due letture del PTY.
fn decode(pending: &mut Vec<u8>, eof: bool) -> String {
    let mut result = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                result.push_str(text);
                pending.clear();
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                result.push_str(
                    std::str::from_utf8(&pending[..valid]).expect("Prefisso UTF-8 valido"),
                );
                pending.drain(..valid);
                if let Some(length) = error.error_len() {
                    result.push('\u{fffd}');
                    pending.drain(..length);
                } else {
                    if eof {
                        result.push('\u{fffd}');
                        pending.clear();
                    }
                    break;
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_frammentato_e_coda_invalida() {
        let mut pending = vec![0xe2, 0x82];
        assert_eq!(decode(&mut pending, false), "");
        pending.push(0xac);
        assert_eq!(decode(&mut pending, false), "€");
        pending.extend([0xff, 0xe2]);
        assert_eq!(decode(&mut pending, true), "��");
    }
}
