use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::models::{ClientId, Message, Order};

#[derive(Debug)]
pub enum DecoderTaskControl {
    ClientAdded(ClientId, OwnedReadHalf),
}

#[derive(Debug)]
pub enum DecoderEvent {
    ClientDisconnected(ClientId),
    Order(ClientId, Order),
}

#[derive(Debug, Default)]
pub struct Decoder {
    clients: HashMap<ClientId, Lines<BufReader<OwnedReadHalf>>>,
}

struct DecoderMessage {
    disconnected_clients: Vec<ClientId>,
    message: Option<(ClientId, Order)>,
}

pub enum ClientDecodeResult {
    Ok(Order),
    SocketError(std::io::Error),
    ClientDisconnected,
}

impl Decoder {
    fn add_client(&mut self, client_id: ClientId, read: OwnedReadHalf) {
        let buf_reader = BufReader::new(read);
        self.clients.insert(client_id, buf_reader.lines());
    }

    async fn next_message_client(
        client_id: &ClientId,
        lines: &mut Lines<BufReader<OwnedReadHalf>>,
    ) -> (ClientId, ClientDecodeResult) {
        loop {
            let next_line = match lines.next_line().await {
                Ok(Some(line)) => line,
                Ok(None) => return (*client_id, ClientDecodeResult::ClientDisconnected),
                Err(e) => return (*client_id, ClientDecodeResult::SocketError(e)),
            };

            let order = match Order::from_str(&next_line) {
                Ok(o) => o,
                Err(e) => {
                    tracing::warn!("Invalid request from {:?}: {:?}", client_id, e);
                    continue;
                }
            };

            return (*client_id, ClientDecodeResult::Ok(order));
        }
    }

    async fn decode_message(&mut self) -> anyhow::Result<DecoderMessage> {
        if self.clients.is_empty() {
            tokio::task::yield_now().await;
            return Ok(DecoderMessage {
                disconnected_clients: Vec::new(),
                message: None,
            });
        }
        let mut futures = FuturesUnordered::new();
        for (client_id, lines) in &mut self.clients {
            futures.push(Self::next_message_client(client_id, lines));
        }

        let mut disconnected_clients = Vec::new();

        loop {
            let Some((client_id, result)) = futures.next().await else {
                tracing::info!("We have ran out of futures");
                break;
            };

            match result {
                ClientDecodeResult::Ok(order) => {
                    return Ok(DecoderMessage {
                        disconnected_clients,
                        message: Some((client_id, order)),
                    });
                }
                ClientDecodeResult::SocketError(_error) => {
                    // There are cases where we could move on. For now disconnect
                    disconnected_clients.push(client_id);
                }
                ClientDecodeResult::ClientDisconnected => {
                    disconnected_clients.push(client_id);
                }
            }
        }

        Ok(DecoderMessage {
            disconnected_clients,
            message: None,
        })
    }

    pub async fn run(
        &mut self,
        mut receiver: Receiver<DecoderTaskControl>,
        sender: Sender<DecoderEvent>,
    ) -> anyhow::Result<()> {
        tracing::info!("Decoder started");
        loop {
            tokio::select! {
                message = receiver.recv() => {
                    if let Some(m) = message {
                        tracing::debug!("Decoder: {:?}", m);
                        match m {
                            DecoderTaskControl::ClientAdded(client_id, read) => {
                                self.add_client(client_id, read);
                            }
                        }
                    } else {
                        tracing::info!("Decoder: Channel closed");
                        return Ok(());
                    }
                }
                message = self.decode_message() => {
                    let DecoderMessage { disconnected_clients, message } = match message {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::error!("Error decoding message: {:?}", e);
                            continue;
                        }
                    };

                    for client_id in disconnected_clients {
                        sender.send(DecoderEvent::ClientDisconnected(client_id)).await?;
                        self.clients.remove(&client_id);
                    }

                    if let Some((client_id, order)) = message {
                        tracing::info!("Message from client {client_id:?}: {order:?}");
                        sender.send(DecoderEvent::Order(client_id, order)).await?;
                    }
                }
            }
        }
    }
}
