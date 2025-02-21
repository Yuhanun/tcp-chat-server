use std::collections::HashMap;

use anyhow::Context;
use tokio::{io::AsyncWriteExt, net::tcp::OwnedWriteHalf, sync::mpsc::Receiver};

use crate::{
    matcher::Match,
    models::{ClientId, Encode, Login, OrderAck, Trade},
};

#[derive(Debug)]
pub enum EncoderTaskControl {
    ClientAdded(ClientId, OwnedWriteHalf),
    ClientDisconnected(ClientId),
    OrderAck(ClientId, OrderAck),
    Match(Match),
}

#[derive(Debug, Default)]
pub struct Encoder {
    clients: HashMap<ClientId, OwnedWriteHalf>,
}

impl Drop for Encoder {
    fn drop(&mut self) {
        futures::executor::block_on(async {
            self.shutdown().await;
        });
    }
}

impl Encoder {
    async fn shutdown(&mut self) {
        tracing::info!("Encoder: Shutdown");
        let iter = self
            .clients
            .drain()
            .map(|(client_id, mut write)| async move {
                tracing::info!("Sending shutdown to {client_id:?}");
                write.shutdown().await
            });

        // We do not care if it fails. We are shutting down anyway
        futures::future::join_all(iter).await;
    }

    fn add_client(&mut self, client_id: ClientId, write: OwnedWriteHalf) {
        self.clients.insert(client_id, write);
    }

    async fn send<T: Encode>(message: &T, writer: &mut OwnedWriteHalf) -> anyhow::Result<()> {
        let mut buffer = [0; 1024];
        let length = message.encode(&mut buffer)?;
        let sent_length = writer.write(&buffer[..length]).await?;
        anyhow::ensure!(
            sent_length == length,
            "Expected to send {length} bytes but sent {sent_length}",
        );

        Ok(())
    }

    async fn on_new_connection(
        &mut self,
        client_id: ClientId,
        mut write: OwnedWriteHalf,
    ) -> anyhow::Result<()> {
        let login = Login { client_id };
        tracing::info!("Sending login message to client: {login:?}");
        Self::send(&login, &mut write).await?;
        self.add_client(client_id, write);

        Ok(())
    }

    async fn handle_control_message(
        &mut self,
        message: Option<EncoderTaskControl>,
    ) -> anyhow::Result<()> {
        if let Some(m) = message {
            tracing::debug!("Encoder: {:?}", m);
            match m {
                EncoderTaskControl::ClientAdded(client_id, write) => {
                    match self.on_new_connection(client_id, write).await {
                        Ok(()) => {
                            tracing::info!("Client {:?} added", client_id);
                        }
                        Err(e) => {
                            tracing::error!("Failed to add {client_id:?}: {e:?}");
                        }
                    }
                }
                EncoderTaskControl::ClientDisconnected(client_id) => {
                    self.clients.remove(&client_id);
                }
                EncoderTaskControl::OrderAck(client_id, order_ack) => {
                    let client = self
                        .clients
                        .get_mut(&client_id)
                        .context("Client not found")?;

                    Self::send(&order_ack, client).await?;
                }
                EncoderTaskControl::Match(m) => {
                    for (_, write) in &mut self.clients {
                        let trade = Trade { product: m.product };
                        Self::send(&trade, write).await?;
                    }
                }
            }
        } else {
            tracing::info!("Encoder: Channel closed");
            anyhow::bail!("Channel closed");
        }

        Ok(())
    }

    pub async fn run(&mut self, mut receiver: Receiver<EncoderTaskControl>) -> anyhow::Result<()> {
        tracing::info!("Encoder started");
        loop {
            tracing::info!("Encoder - Waiting for message...");
            tokio::select! {
                biased;
                message = receiver.recv() =>  {
                    self.handle_control_message(message).await?;
                }
                // message = server_event_receiver.recv() => {
                //     self.handle_server_message(message).await?;
                // }
            }
        }
    }
}
