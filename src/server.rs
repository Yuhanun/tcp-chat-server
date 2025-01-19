use std::{fmt::Debug, net::SocketAddr};

use anyhow::Context;
use tokio::{net::ToSocketAddrs, sync::mpsc::Sender};
use tokio_util::sync::CancellationToken;

use crate::{decoder::DecoderTaskControl, encoder::EncoderTaskControl, models::ClientId};

#[derive(Debug)]
pub struct Server {
    listener: tokio::net::TcpListener,
}

impl Server {
    pub async fn bind<T: ToSocketAddrs + Debug + Send>(addr: T) -> anyhow::Result<Self> {
        tracing::info!("Starting server on {addr:?}");
        Ok(Self {
            listener: tokio::net::TcpListener::bind(addr).await?,
        })
    }

    async fn handle_new_client(
        &self,
        stream: tokio::net::TcpStream,
        socket: SocketAddr,
        encoder_sender: Sender<EncoderTaskControl>,
        decoder_sender: Sender<DecoderTaskControl>,
    ) -> anyhow::Result<()> {
        tracing::info!("Accepted connection from: {:?}", socket);
        match stream.writable().await {
            Ok(()) => {}
            Err(e) => {
                tracing::error!("Failed to wait for stream to be writable: {:?}", e);
                return Ok(());
            }
        }
        stream.set_nodelay(true)?;
        let (read, write) = stream.into_split();
        let client_id = ClientId(socket.port());
        decoder_sender
            .send(DecoderTaskControl::ClientAdded(client_id, read))
            .await
            .context("Failed to send message to decoder")?;
        encoder_sender
            .send(EncoderTaskControl::ClientAdded(client_id, write))
            .await
            .context("Failed to send message to encoder")?;

        Ok(())
    }

    pub async fn run(
        &self,
        encoder_sender: Sender<EncoderTaskControl>,
        decoder_sender: Sender<DecoderTaskControl>,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::info!("Server started");
        loop {
            tracing::info!("Waiting for connection...");
            tokio::select! {
                () = cancellation_token.cancelled() => {
                    tracing::info!("Server cancelled");
                    return Ok(());
                }
                client = self.listener.accept() => {
                    match client {
                        Ok((stream, socket)) => {
                            match self.handle_new_client(stream, socket, encoder_sender.clone(), decoder_sender.clone()).await {
                                Ok(()) => {}
                                Err(e) => {
                                    tracing::error!("Failed to handle new client: {e:?}");
                                }
                            };
                        }
                        Err(e) => {
                            tracing::error!("Failed to accept connection: {e:?}");
                        }
                    }
                }
            }
        }
    }
}
