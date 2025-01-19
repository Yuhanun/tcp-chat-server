use single_thread_async_server::{
    decoder::{Decoder, DecoderEvent, DecoderTaskControl},
    encoder::{Encoder, EncoderTaskControl},
    server::Server,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
struct TcpClient {
    line_reader: Lines<BufReader<OwnedReadHalf>>,
    writer: OwnedWriteHalf,
}

impl TcpClient {
    pub async fn connect(address: &str) -> Self {
        Self::try_connect(address).await.expect("Failed to connect")
    }

    pub async fn try_connect(address: &str) -> anyhow::Result<Self> {
        let tcp_socket = TcpStream::connect(address).await?;
        let (read, writer) = tcp_socket.into_split();
        let line_reader = BufReader::new(read).lines();

        Ok(TcpClient {
            line_reader,
            writer,
        })
    }

    async fn read_line(&mut self) -> anyhow::Result<Option<String>> {
        let line = match self.line_reader.next_line().await {
            Ok(line) => Ok(line),
            Err(e) => Err(anyhow::anyhow!(e)),
        };

        tracing::debug!("Received line: {:?}", line);

        line
    }

    async fn write_line(&mut self, line: &str) -> anyhow::Result<()> {
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;

        // NOTE: .next_line() wipes the newline
        const EXPECTED_ACK: &str = "ACK:MESSAGE";
        let line = self.read_line().await?;
        match line {
            Some(line) => {
                if line == EXPECTED_ACK {
                    return Ok(());
                }
                let line_bytes = line.chars();
                let expected_bytes = EXPECTED_ACK.chars();
                anyhow::bail!(
                    "Expected {EXPECTED_ACK} ({line_bytes:?}), got: {line} ({expected_bytes:?})",
                );
            }
            None => Err(anyhow::anyhow!("Expected a line")),
        }
    }

    async fn verify_login(&mut self) -> anyhow::Result<()> {
        let line = self.read_line().await?;
        match line {
            Some(line) => {
                regex_matches(&line, LOGIN_REGEX_MATCH)?;
                Ok(())
            }
            None => Err(anyhow::anyhow!("Expected a line")),
        }
    }
}

struct TestServerHandle {
    pub server: Server,
    pub encoder: Encoder,
    pub decoder: Decoder,
}

async fn create_server(port: u16) -> anyhow::Result<TestServerHandle> {
    let server = Server::bind(("0.0.0.0", port)).await?;
    let encoder = Encoder::default();
    let decoder = Decoder::default();

    Ok(TestServerHandle {
        server,
        encoder,
        decoder,
    })
}

async fn run_all(
    handle: TestServerHandle,
) -> anyhow::Result<(Vec<JoinHandle<anyhow::Result<()>>>, CancellationToken)> {
    let mut futures = Vec::new();
    let (encoder_sender, encoder_receiver) =
        tokio::sync::mpsc::channel::<EncoderTaskControl>(u8::MAX as usize);
    let (decoder_sender, decoder_receiver) =
        tokio::sync::mpsc::channel::<DecoderTaskControl>(u8::MAX as usize);
    let (decoder_event_sender, decoder_event_receiver) =
        tokio::sync::mpsc::channel::<DecoderEvent>(u8::MAX as usize);

    let cancellation_token = CancellationToken::new();
    let cloned_cancellation_token = cancellation_token.clone();

    let mut encoder = handle.encoder;
    let mut decoder = handle.decoder;
    let server = handle.server;

    let encoder_fut =
        tokio::spawn(async move { encoder.run(encoder_receiver, decoder_event_receiver).await });
    let decoder_fut =
        tokio::spawn(async move { decoder.run(decoder_receiver, decoder_event_sender).await });
    let server_fut = tokio::spawn(async move {
        server
            .run(encoder_sender, decoder_sender, cloned_cancellation_token)
            .await
    });

    futures.push(encoder_fut);
    futures.push(decoder_fut);
    futures.push(server_fut);

    Ok((futures, cancellation_token))
}

async fn stop_all(
    futures: Vec<JoinHandle<anyhow::Result<()>>>,
    cancellation_token: CancellationToken,
) -> anyhow::Result<()> {
    cancellation_token.cancel();

    // We don't care if they exit gracefully or not in tests
    let _ = futures::future::join_all(futures.into_iter()).await;

    Ok(())
}

const LOGIN_REGEX_MATCH: &str = r"LOGIN:\d+";

fn regex_matches(line: &str, regex: &str) -> anyhow::Result<()> {
    let re = regex::Regex::new(regex)?;
    anyhow::ensure!(re.is_match(line), "Line did not match regex: {}", line);

    Ok(())
}

#[tokio::test]
async fn test_login() {
    let handle = create_server(9000).await.expect("Failed to create server");
    let (futures, cancellation_token) = run_all(handle).await.expect("Failed to run server");

    let mut client1 = TcpClient::connect("0.0.0.0:9000").await;

    client1
        .verify_login()
        .await
        .expect("Failed to verify login");

    stop_all(futures, cancellation_token)
        .await
        .expect("Failed to stop server");
}

const HELLO_WORLD_MESSAGE_RECEIVED: &str = r"MESSAGE:\d+ Hello, World!";

#[tokio::test]
async fn test_messaging() {
    let handle = create_server(9001).await.expect("Failed to create server");
    let (futures, cancellation_token) = run_all(handle).await.expect("Failed to run server");

    let mut client1 = TcpClient::connect("0.0.0.0:9001").await;
    client1
        .verify_login()
        .await
        .expect("Failed to verify login");

    let mut client2 = TcpClient::connect("0.0.0.0:9001").await;
    client2
        .verify_login()
        .await
        .expect("Failed to verify login");

    let mut client3 = TcpClient::connect("0.0.0.0:9001").await;
    client3
        .verify_login()
        .await
        .expect("Failed to verify login");

    client1
        .write_line("Hello, World!")
        .await
        .expect("Failed to write message");

    let line_client2 = client2.read_line().await.expect("Failed to read message");
    let line_client3 = client3.read_line().await.expect("Failed to read message");

    let line_client2 = match line_client2 {
        Some(line) => {
            regex_matches(&line, HELLO_WORLD_MESSAGE_RECEIVED).expect("Failed to match message");
            line
        }
        None => panic!("Expected a line"),
    };

    let line_client3 = match line_client3 {
        Some(line) => {
            regex_matches(&line, HELLO_WORLD_MESSAGE_RECEIVED).expect("Failed to match message");
            line
        }
        None => panic!("Expected a line"),
    };

    assert_eq!(line_client2, line_client3);

    stop_all(futures, cancellation_token)
        .await
        .expect("Failed to stop server");
}
