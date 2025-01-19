use std::collections::HashMap;

use anyhow::Context;
use tokio::{io::AsyncWriteExt, net::tcp::OwnedWriteHalf, sync::mpsc::Receiver};

use crate::{
    decoder::DecoderEvent,
    models::{ClientId, Encode, Login, MessageAck},
};

#[derive(Debug)]
pub enum EncoderTaskControl {
    ClientAdded(ClientId, OwnedWriteHalf),
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
            }
        } else {
            tracing::info!("Encoder: Channel closed");
            anyhow::bail!("Channel closed");
        }

        Ok(())
    }

    async fn handle_decoder_message(
        &mut self,
        message: Option<DecoderEvent>,
    ) -> anyhow::Result<()> {
        match message {
            Some(DecoderEvent::ClientDisconnected(client_id)) => {
                self.clients.remove(&client_id);
            }
            Some(DecoderEvent::ClientMessage(message)) => {
                for (other_client_id, write) in &mut self.clients {
                    if *other_client_id == message.origin_client_id {
                        continue;
                    }
                    tracing::info!(
                        "Sending message to client {:?}: {}",
                        other_client_id,
                        message.message
                    );

                    // Note: We encode for every client here, might be worth improving in the future
                    Self::send(&message, write).await?;
                }

                let write = self
                    .clients
                    .get_mut(&message.origin_client_id)
                    .context("Client not found")?;
                Self::send(&MessageAck, write).await?;
            }
            None => {
                tracing::info!("Encoder: Channel closed");
                anyhow::bail!("Channel closed");
            }
        }

        Ok(())
    }

    pub async fn run(
        &mut self,
        mut receiver: Receiver<EncoderTaskControl>,
        mut decoder_event_receiver: Receiver<DecoderEvent>,
    ) -> anyhow::Result<()> {
        tracing::info!("Encoder started");
        loop {
            tracing::info!("Encoder - Waiting for message...");
            tokio::select! {
                biased;
                message = receiver.recv() =>  {
                    self.handle_control_message(message).await?;
                }
                message = decoder_event_receiver.recv() => {
                    self.handle_decoder_message(message).await?;
                }
            }
        }
    }
}
