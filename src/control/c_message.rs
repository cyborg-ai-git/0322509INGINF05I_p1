//! Codifica JSON del token e collegamento fra dominio e trasporto TCP.

use crate::bridge::BTcp;
use crate::entity::EToken;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};

/// Codifica, validazione e trasporto dei messaggi contenenti il token.
pub struct CMessage;

impl CMessage {
    /// Serializza il token come JSON seguito da una newline.
    ///
    /// # Errori
    /// Propaga eventuali errori di Serde. Non valida il dominio; il binario deve
    /// inviare soltanto il token già validato e prodotto dall’elaborazione locale.
    pub fn encode_token(token: &EToken) -> Result<String, serde_json::Error> {
        let mut encoded = serde_json::to_string(token)?;
        encoded.push('\n');
        Ok(encoded)
    }

    /// Deserializza un messaggio JSON e ne valida i campi di dominio.
    ///
    /// # Errori
    /// Rifiuta JSON malformato, campi inattesi e token semanticamente invalidi.
    /// La delimitazione TCP è verificata separatamente da [`Self::read_token`].
    pub fn decode_token(line: &str) -> Result<EToken, serde_json::Error> {
        let token: EToken = serde_json::from_str(line.trim())?;
        crate::CToken::validate(&token).map_err(<serde_json::Error as serde::de::Error>::custom)?;
        Ok(token)
    }

    /// Legge un messaggio completo da una connessione TCP già accettata.
    ///
    /// # Errori
    /// Propaga errori I/O e scadenze del trasporto; restituisce `InvalidData` per
    /// errori JSON o di dominio. Il chiamante conserva il listener e può continuare.
    pub async fn read_token(stream: TcpStream) -> io::Result<EToken> {
        let line = BTcp::read_line(stream).await?;
        Self::decode_token(&line).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Legge e valida un token mantenendo il canale del predecessore fra più giri.
    ///
    /// # Errori
    /// Propaga gli errori di [`BTcp::read_next_line`] e di dominio. `None` indica
    /// una chiusura fra frame; il chiamante può tornare ad accettare connessioni.
    pub async fn read_next_token(reader: &mut BufReader<TcpStream>) -> io::Result<Option<EToken>> {
        BTcp::read_next_line(reader)
            .await?
            .map(|line| {
                Self::decode_token(&line)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
            })
            .transpose()
    }

    /// Trasferisce il solo token posseduto riutilizzando il canale verso il successore.
    ///
    /// # Errori
    /// Propaga errori di codifica e di [`BTcp::send_reusing_connection`]. Il binario
    /// deve arrestarsi dopo un errore di scrittura senza ricreare o reinviare il token.
    pub async fn send_token_reusing_connection(
        connection: &mut Option<TcpStream>,
        successor: SocketAddr,
        token: &EToken,
        retry_delay: Duration,
    ) -> io::Result<()> {
        let encoded = Self::encode_token(token)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        BTcp::send_reusing_connection(connection, successor, &encoded, retry_delay).await
    }

    /// Apre una connessione al successore e invia il token una sola volta.
    ///
    /// # Errori
    /// Propaga errori di codifica, connessione, scrittura e chiusura. Un esito
    /// positivo indica completamento del trasporto, non conferma applicativa remota.
    pub async fn send_token_once(successor: SocketAddr, token: &EToken) -> io::Result<()> {
        let encoded = Self::encode_token(token)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        BTcp::send_once(successor, &encoded).await
    }

    /// Invia il token ritentando soltanto la connessione prima della scrittura.
    ///
    /// La politica dettagliata è in [`BTcp::send_with_retry`]. L’attesa di un
    /// successore assente può continuare senza limite complessivo.
    ///
    /// # Errori
    /// Propaga errori di codifica, connessione non ritentabile e scrittura incerta.
    /// Il chiamante deve interrompersi senza reinviare o rigenerare il token.
    pub async fn send_token_with_retry(
        successor: SocketAddr,
        token: &EToken,
        retry_delay: Duration,
    ) -> io::Result<()> {
        let encoded = Self::encode_token(token)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        BTcp::send_with_retry(successor, &encoded, retry_delay).await
    }

    /// Apre il listener privato di un processo ATM sull’interfaccia locale.
    ///
    /// # Errori
    /// Propaga gli errori di [`BTcp::bind_listener`], inclusa una porta già occupata.
    pub async fn bind_listener(bind: SocketAddr) -> io::Result<TcpListener> {
        BTcp::bind_listener(bind).await
    }
}
