use std::{fmt::Debug, net::SocketAddr};

use anyhow::Context;
use tokio::{
    net::ToSocketAddrs,
    sync::mpsc::{Receiver, Sender},
};
use tokio_util::sync::CancellationToken;

use crate::{
    decoder::{DecoderEvent, DecoderTaskControl},
    encoder::EncoderTaskControl,
    matcher::Matcher,
    models::{ClientId, Message, OrderAck, Side, Trade},
};

#[derive(Debug)]
pub struct Server {
    listener: tokio::net::TcpListener,

    // Cell
    matcher: Matcher,
}

impl Server {
    pub async fn bind<T: ToSocketAddrs + Debug + Send>(addr: T) -> anyhow::Result<Self> {
        tracing::info!("Starting server on {addr:?}");
        Ok(Self {
            listener: tokio::net::TcpListener::bind(addr).await?,
            matcher: Matcher::new(),
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

    // Mutable TODO
    async fn handle_decoder_event(
        &mut self,
        msg: DecoderEvent,
        encoder_sender: &Sender<EncoderTaskControl>,
    ) -> anyhow::Result<()> {
        match msg {
            DecoderEvent::ClientDisconnected(client_id) => {
                // forward the event
                encoder_sender
                    .send(EncoderTaskControl::ClientDisconnected(client_id))
                    .await?;

                Ok(())
            }
            DecoderEvent::Order(client_id, order) => {
                encoder_sender
                    .send(EncoderTaskControl::OrderAck(
                        client_id,
                        OrderAck {
                            product: order.product,
                        },
                    ))
                    .await?;

                let trade_opt = match order.side {
                    Side::Buy => self.matcher.add_buy(order.product),
                    Side::Sell => self.matcher.add_sell(order.product),
                };

                if let Some(t) = trade_opt {
                    encoder_sender.send(EncoderTaskControl::Match(t)).await?;
                }

                Ok(())
            }
        }
    }

    pub async fn run(
        &mut self,
        encoder_sender: Sender<EncoderTaskControl>,
        decoder_sender: Sender<DecoderTaskControl>,
        mut decoder_event_receiver: Receiver<DecoderEvent>,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::info!("Server started");
        loop {
            tracing::info!("Waiting for connection...");
            tokio::select! {
                biased;
                decoder_event = decoder_event_receiver.recv() => {
                    match decoder_event {
                        None => {},
                        Some(msg) => {
                            self.handle_decoder_event(msg, &encoder_sender).await?;
                        }
                    }

                }
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
