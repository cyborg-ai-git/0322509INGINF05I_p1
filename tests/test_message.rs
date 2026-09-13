//! Codec JSON e trasporto TCP reale: delimitazione, limiti e scadenze.

use app_atm::{CAtm, CMessage, CToken};
use tokio::net::TcpListener;

/// La codifica termina con newline e la decodifica ricostruisce i dati.
#[test]
fn token_codec_round_trips_as_newline_delimited_json() {
    let mut token = CToken::create_initial(1234);
    let atm = CAtm::build("ATM2").expect("valid ATM");
    CToken::mark_processed_by(&mut token, &atm).expect("valid token metadata");

    let encoded = CMessage::encode_token(&token).expect("token encodes");
    assert!(encoded.ends_with('\n'));

    let decoded = CMessage::decode_token(&encoded).expect("token decodes");
    assert_eq!(decoded, token);
}

/// Un listener e un mittente reali verificano un passaggio TCP locale.
#[tokio::test]
async fn token_is_sent_and_received_over_localhost_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ephemeral listener");
    let address = listener.local_addr().expect("listener address");
    let token = CToken::create_initial(1000);
    let expected = token.clone();

    let receiver = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept token");
        CMessage::read_token(stream).await.expect("read token")
    });

    CMessage::send_token_once(address, &token)
        .await
        .expect("send token");

    let received = receiver.await.expect("receiver task");
    assert_eq!(received, expected);
}

/// Distingue JSON valido da token ammissibile e rifiuta entrambi i tipi di errore.
#[test]
fn malformed_and_semantically_invalid_tokens_are_rejected() {
    for raw in [
        "",
        "{}",
        "null",
        "{bad json}",
        r#"{"balance":-1,"hop":0,"last_holder":null}"#,
        r#"{"balance":1,"hop":18446744073709551615,"last_holder":null}"#,
        r#"{"balance":1,"hop":1,"last_holder":{"id":"ATM9"}}"#,
        r#"{"balance":1,"hop":0,"last_holder":null,"extra":1}"#,
    ] {
        assert!(CMessage::decode_token(raw).is_err(), "{raw}");
    }
}

async fn receive_bytes(bytes: Vec<u8>) -> std::io::Result<String> {
    use tokio::io::AsyncWriteExt;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let sender = tokio::spawn(async move {
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        // Frammenta il messaggio per verificare la ricomposizione del flusso TCP.
        for chunk in bytes.chunks(7) {
            if stream.write_all(chunk).await.is_err() {
                break;
            }
        }
    });
    let (stream, _) = listener.accept().await.unwrap();
    let result = app_atm::BTcp::read_line(stream).await;
    sender.await.unwrap();
    result
}

/// Verifica frammentazione TCP, dimensione massima, UTF-8 e terminatore obbligatorio.
#[tokio::test]
async fn transport_handles_fragmentation_and_requires_complete_bounded_utf8() {
    let encoded = CMessage::encode_token(&CToken::create_initial(1000)).unwrap();
    assert_eq!(
        receive_bytes(encoded.clone().into_bytes()).await.unwrap(),
        encoded
    );
    assert!(receive_bytes(b"incomplete".to_vec()).await.is_err());
    assert!(receive_bytes(Vec::new()).await.is_err());
    assert!(receive_bytes(vec![b'x'; 4097]).await.is_err());
    assert!(receive_bytes(vec![255, b'\n']).await.is_err());
    let mut boundary = vec![b'x'; 4095];
    boundary.push(b'\n');
    assert_eq!(receive_bytes(boundary).await.unwrap().len(), 4096);
}

/// Una connessione incompleta scade entro la durata configurata dalla prova.
#[tokio::test]
async fn stalled_connection_expires_without_waiting_forever() {
    use std::time::Duration;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let _client = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (stream, _) = listener.accept().await.unwrap();
    let error = app_atm::BTcp::read_line_with_timeout(stream, Duration::from_millis(20))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
}

/// Ritenta la connessione prima della scrittura e consegna un solo messaggio.
#[tokio::test]
async fn connection_retry_delivers_one_frame_when_successor_starts_late() {
    use std::time::Duration;
    let reservation = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let sender = tokio::spawn(async move {
        CMessage::send_token_with_retry(
            address,
            &CToken::create_initial(1000),
            Duration::from_millis(5),
        )
        .await
        .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(25)).await;
    let listener = TcpListener::bind(address).await.unwrap();
    let (stream, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(CMessage::read_token(stream).await.unwrap().balance, 1000);
    sender.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(25), listener.accept())
            .await
            .is_err()
    );
}

/// Ventimila trasferimenti usano una sola connessione e conservano tutti i frame.
#[tokio::test]
async fn ventimila_frame_sulla_stessa_connessione_senza_perdite() {
    use std::time::Duration;
    use tokio::io::BufReader;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let sender = async {
        let mut connection = None;
        let mut endpoint = None;
        for hop in 0..20_000 {
            let mut token = CToken::create_initial(1000);
            token.hop = hop;
            CMessage::send_token_reusing_connection(
                &mut connection,
                address,
                &token,
                Duration::ZERO,
            )
            .await
            .unwrap();
            let local = connection.as_ref().unwrap().local_addr().unwrap();
            assert_eq!(*endpoint.get_or_insert(local), local);
        }
    };
    let receiver = async {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        for hop in 0..20_000 {
            let token = CMessage::read_next_token(&mut reader)
                .await
                .unwrap()
                .unwrap();
            assert_eq!((token.hop, token.balance), (hop, 1000));
        }
        assert!(
            CMessage::read_next_token(&mut reader)
                .await
                .unwrap()
                .is_none()
        );
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(sender, receiver);
    })
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(25), listener.accept())
            .await
            .is_err()
    );
}

/// Il tempo passato in attesa del token altrove non è un timeout del frame.
#[tokio::test]
async fn canale_persistente_attende_il_token_oltre_la_scadenza_del_frame() {
    use std::time::Duration;
    use tokio::io::{AsyncWriteExt, BufReader};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (stream, _) = listener.accept().await.unwrap();
    let mut reader = BufReader::new(stream);
    let sender = async {
        tokio::time::sleep(app_atm::bridge::b_tcp::IO_TIMEOUT + Duration::from_millis(100)).await;
        let frame = CMessage::encode_token(&CToken::create_initial(1000)).unwrap();
        for part in frame.as_bytes().chunks(7) {
            client.write_all(part).await.unwrap();
        }
    };
    let (_, received) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(sender, CMessage::read_next_token(&mut reader))
    })
    .await
    .unwrap();
    assert_eq!(received.unwrap().unwrap().balance, 1000);
}

/// Limiti e validazione si applicano anche ai frame di una connessione persistente.
#[tokio::test]
async fn canale_persistente_rifiuta_frame_troppo_lunghi_corrotti_o_troncati() {
    use tokio::io::{AsyncWriteExt, BufReader};
    for (bytes, kind) in [
        (vec![b'x'; 4097], std::io::ErrorKind::InvalidData),
        (vec![255, b'\n'], std::io::ErrorKind::InvalidData),
        (b"senza newline".to_vec(), std::io::ErrorKind::UnexpectedEof),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        client.write_all(&bytes).await.unwrap();
        drop(client);
        let (stream, _) = listener.accept().await.unwrap();
        let error = app_atm::BTcp::read_next_line(&mut BufReader::new(stream))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), kind);
    }
}

/// Dopo il primo byte, un frame che non termina deve scadere anche su un canale aperto.
#[tokio::test]
async fn frame_persistente_iniziato_ma_incompleto_scade() {
    use std::{io::ErrorKind, time::Duration};
    use tokio::io::{AsyncWriteExt, BufReader};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    client.write_all(b"{").await.unwrap();
    let (stream, _) = listener.accept().await.unwrap();
    let error = tokio::time::timeout(
        Duration::from_secs(4),
        CMessage::read_next_token(&mut BufReader::new(stream)),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TimedOut);
}
