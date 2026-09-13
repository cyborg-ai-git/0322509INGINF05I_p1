//! Trasporto TCP locale con delimitazione, limiti di lettura e invio senza duplicazioni.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

/// Dimensione massima di un messaggio UTF-8, inclusa la newline finale.
pub const MAX_FRAME_BYTES: usize = 4096;
/// Tempo massimo di una fase I/O; il suo scadere non autorizza a ricreare il token.
pub const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// Adattatore TCP per frame terminati da newline, anche su connessioni riutilizzate.
pub struct BTcp;

impl BTcp {
    /// Apre un listener su un indirizzo di loopback.
    ///
    /// # Errori
    /// Restituisce `InvalidInput` per indirizzi esterni e propaga gli errori di bind.
    /// La restrizione applicativa esatta a `127.0.0.1` è in [`crate::CConfig`].
    pub async fn bind_listener(bind: SocketAddr) -> io::Result<TcpListener> {
        if !bind.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "è richiesto un indirizzo di loopback",
            ));
        }
        TcpListener::bind(bind).await
    }

    /// Legge un messaggio delimitato, entro [`MAX_FRAME_BYTES`] e [`IO_TIMEOUT`].
    ///
    /// # Errori
    /// Si applicano gli stessi errori di [`Self::read_line_with_timeout`].
    pub async fn read_line(stream: TcpStream) -> io::Result<String> {
        Self::read_line_with_timeout(stream, IO_TIMEOUT).await
    }

    /// Ricompone il flusso TCP fino alla prima newline entro la durata indicata.
    ///
    /// Legge al massimo un byte oltre il limite, così identifica messaggi troppo
    /// grandi senza allocazioni illimitate. Le letture possono essere frammentate.
    ///
    /// # Errori
    /// Restituisce `InvalidData` per dimensione o UTF-8 invalidi, `UnexpectedEof`
    /// per un messaggio non terminato e `TimedOut` alla scadenza, oltre agli errori I/O.
    pub async fn read_line_with_timeout(
        stream: TcpStream,
        deadline: Duration,
    ) -> io::Result<String> {
        let read = async {
            let mut reader = BufReader::new(stream.take((MAX_FRAME_BYTES + 1) as u64));
            let mut bytes = Vec::new();
            reader.read_until(b'\n', &mut bytes).await?;
            if bytes.len() > MAX_FRAME_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "il messaggio supera 4096 byte",
                ));
            }
            if bytes.last() != Some(&b'\n') {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "messaggio incompleto: manca la newline finale",
                ));
            }
            String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        };
        timeout(deadline, read).await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "tempo massimo di lettura del messaggio superato",
            )
        })?
    }

    /// Legge il prossimo frame mantenendo il buffer della connessione fra le visite.
    ///
    /// L’attesa del primo byte è libera: il token può essere trattenuto altrove
    /// durante una transazione. Dal primo byte il frame deve completarsi entro
    /// [`IO_TIMEOUT`]. Un EOF fra frame restituisce `None`; non crea un nuovo token.
    ///
    /// # Errori
    /// Rifiuta frame oltre 4096 byte, UTF-8 invalido, EOF parziale e timeout del
    /// frame iniziato. Dopo un errore il chiamante deve scartare la connessione.
    pub async fn read_next_line(reader: &mut BufReader<TcpStream>) -> io::Result<Option<String>> {
        if reader.fill_buf().await?.is_empty() {
            return Ok(None);
        }
        let read = async {
            let mut bytes = Vec::new();
            (&mut *reader)
                .take((MAX_FRAME_BYTES + 1) as u64)
                .read_until(b'\n', &mut bytes)
                .await?;
            if bytes.len() > MAX_FRAME_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "il messaggio supera 4096 byte",
                ));
            }
            if bytes.last() != Some(&b'\n') {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "messaggio incompleto: manca la newline finale",
                ));
            }
            String::from_utf8(bytes)
                .map(Some)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        };
        timeout(IO_TIMEOUT, read).await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "tempo massimo di lettura del messaggio superato",
            )
        })?
    }

    /// Invia un frame sulla stessa connessione al successore, aprendola al primo uso.
    ///
    /// Il riuso evita una nuova porta effimera a ogni giro. `TCP_NODELAY` permette
    /// l’invio dei piccoli frame senza attese di aggregazione. La connessione resta
    /// aperta; il completamento della scrittura non è una conferma applicativa remota.
    ///
    /// # Errori
    /// Ritenta solo l’apertura iniziale. Rifiuta un socket associato a un altro peer;
    /// qualsiasi errore dopo l’inizio della scrittura va propagato senza ritrasmissione.
    pub async fn send_reusing_connection(
        connection: &mut Option<TcpStream>,
        successor: SocketAddr,
        message: &str,
        retry_delay: Duration,
    ) -> io::Result<()> {
        if connection.is_none() {
            let stream =
                Self::connect_with_connector(successor, retry_delay, Self::connect).await?;
            stream.set_nodelay(true)?;
            *connection = Some(stream);
        }
        let stream = connection.as_mut().expect("Connessione inizializzata");
        if stream.peer_addr()? != successor {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "connessione associata a un altro successore",
            ));
        }
        timeout(IO_TIMEOUT, async {
            stream.write_all(message.as_bytes()).await?;
            stream.flush().await
        })
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "consegna del token incerta: arresto senza ritrasmissione",
            )
        })?
    }

    /// Connette al solo loopback entro la scadenza della fase di apertura.
    async fn connect(address: SocketAddr) -> io::Result<TcpStream> {
        if !address.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "è richiesto un indirizzo di loopback",
            ));
        }
        timeout(IO_TIMEOUT, TcpStream::connect(address))
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "tempo massimo di connessione superato",
                )
            })?
    }

    /// Scrive e chiude una volta sola; qualsiasi errore successivo alla scrittura è incerto.
    async fn write_frame(mut stream: TcpStream, message: &str) -> io::Result<()> {
        // Dopo l’inizio di write_all la consegna può essere incerta.
        // Il destinatario potrebbe già avere il token: non ripetiamo la scrittura.
        timeout(IO_TIMEOUT, async {
            stream.write_all(message.as_bytes()).await?;
            stream.shutdown().await
        })
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "consegna del token incerta: arresto senza ritrasmissione",
            )
        })?
    }

    /// Connette e scrive una sola volta, con scadenze separate per le due fasi.
    ///
    /// Il successo non certifica che l’applicazione remota abbia elaborato il token.
    ///
    /// # Errori
    /// Propaga errori di indirizzo, connessione, scrittura e chiusura. Dopo l’inizio
    /// della scrittura l’esito può essere incerto e non autorizza una ritrasmissione.
    pub async fn send_once(successor: SocketAddr, message: &str) -> io::Result<()> {
        let stream = Self::connect(successor).await?;
        Self::write_frame(stream, message).await
    }

    ///
    /// Ritenta la sola apertura della connessione prima di scrivere qualsiasi byte.
    ///
    /// Ritenta connessioni rifiutate, scadute, azzerate, non connesse o prive di una
    /// porta locale temporaneamente disponibile. Attende almeno un millisecondo
    /// fra tentativi e non impone una scadenza complessiva, così supporta avvii
    /// sfalsati e disponibilità transitoria delle risorse. Non crea nuovi token.
    ///
    /// # Errori
    /// Gli altri errori di connessione sono propagati. Dopo la prima connessione
    /// riuscita, ogni errore di scrittura o chiusura termina l’invio senza ripeterlo;
    /// il binario arresta il nodo e richiede un nuovo avvio dell’intero anello.
    pub async fn send_with_retry(
        successor: SocketAddr,
        message: &str,
        retry_delay: Duration,
    ) -> io::Result<()> {
        Self::send_with_connector(successor, message, retry_delay, Self::connect).await
    }

    /// Separa l’apertura dalla scrittura: le prove possono introdurre un errore di
    /// connessione senza occupare tutte le porte del sistema operativo. Dopo una
    /// connessione riuscita, il messaggio viene scritto una sola volta.
    async fn send_with_connector<F, Fut>(
        successor: SocketAddr,
        message: &str,
        retry_delay: Duration,
        connect: F,
    ) -> io::Result<()>
    where
        F: FnMut(SocketAddr) -> Fut,
        Fut: std::future::Future<Output = io::Result<TcpStream>>,
    {
        let stream = Self::connect_with_connector(successor, retry_delay, connect).await?;
        Self::write_frame(stream, message).await
    }

    /// Ritenta solo l’apertura; la stessa politica serve a invii singoli e persistenti.
    async fn connect_with_connector<F, Fut>(
        successor: SocketAddr,
        retry_delay: Duration,
        mut connect: F,
    ) -> io::Result<TcpStream>
    where
        F: FnMut(SocketAddr) -> Fut,
        Fut: std::future::Future<Output = io::Result<TcpStream>>,
    {
        let stream = loop {
            match connect(successor).await {
                Ok(stream) => break stream,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionRefused
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::NotConnected
                            | io::ErrorKind::AddrNotAvailable
                    ) =>
                {
                    eprintln!(
                        "Successore {successor}: {error}; nuovo tentativo di connessione tra {} ms",
                        retry_delay.as_millis()
                    );
                    sleep(retry_delay.max(Duration::from_millis(1))).await;
                }
                Err(error) => return Err(error),
            }
        };
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Due indisponibilità locali precedono una connessione TCP reale. La prova
    /// controlla la consegna di un solo frame e l’assenza di una seconda connessione;
    /// non esaurisce le porte del computer e non interviene sui parametri del kernel.
    #[tokio::test]
    async fn porte_temporaneamente_indisponibili_non_perdono_o_duplicano_il_messaggio() {
        let listener = BTcp::bind_listener("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Cell::new(0);
        let frame = "token di prova\n";
        let sender = BTcp::send_with_connector(address, frame, Duration::ZERO, |endpoint| {
            let attempt = attempts.get();
            attempts.set(attempt + 1);
            async move {
                if attempt < 2 {
                    Err(io::Error::from(io::ErrorKind::AddrNotAvailable))
                } else {
                    BTcp::connect(endpoint).await
                }
            }
        });
        let receiver = async {
            let (stream, _) = listener.accept().await.unwrap();
            BTcp::read_line(stream).await.unwrap()
        };
        let (sent, received) = timeout(Duration::from_secs(2), async {
            tokio::join!(sender, receiver)
        })
        .await
        .unwrap();
        sent.unwrap();
        assert_eq!(attempts.get(), 3);
        assert_eq!(received, frame);
        assert!(
            timeout(Duration::from_millis(25), listener.accept())
                .await
                .is_err()
        );

        // Un errore permanente deve invece essere restituito al primo tentativo.
        attempts.set(0);
        let error = BTcp::send_with_connector(address, frame, Duration::ZERO, |_| {
            attempts.set(attempts.get() + 1);
            async { Err(io::Error::from(io::ErrorKind::InvalidInput)) }
        })
        .await
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(attempts.get(), 1);
    }
}
