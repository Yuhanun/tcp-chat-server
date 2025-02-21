#![warn(
    clippy::pedantic,
    clippy::suspicious,
    clippy::perf,
    clippy::style,
    clippy::nursery,
    clippy::expect_used,
    clippy::unwrap_used
)]
#![allow(
    clippy::missing_errors_doc,
    // tokio::select !{} does this internally...
    clippy::redundant_pub_crate
)]
use single_thread_async_server::decoder::{Decoder, DecoderEvent, DecoderTaskControl};
use single_thread_async_server::encoder::{Encoder, EncoderTaskControl};
use single_thread_async_server::server::Server;
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut server = Server::bind("0.0.0.0:8888").await?;
    let mut encoder = Encoder::default();
    let mut decoder = Decoder::default();

    let (encoder_sender, encoder_receiver) =
        tokio::sync::mpsc::channel::<EncoderTaskControl>(u8::MAX as usize);
    let (decoder_sender, decoder_receiver) =
        tokio::sync::mpsc::channel::<DecoderTaskControl>(u8::MAX as usize);
    let (decoder_event_sender, decoder_event_receiver) =
        tokio::sync::mpsc::channel::<DecoderEvent>(u8::MAX as usize);

    let cancellation_token = CancellationToken::new();
    let ctrlc_cancellation_token = cancellation_token.clone();

    let encoder_fut = encoder.run(encoder_receiver);
    let decoder_fut = decoder.run(decoder_receiver, decoder_event_sender);
    let server_fut = server.run(
        encoder_sender,
        decoder_sender,
        decoder_event_receiver,
        cancellation_token.clone(),
    );

    ctrlc::set_handler(move || {
        tracing::warn!("Ctrl-C received, cancelling tasks");
        ctrlc_cancellation_token.cancel();
    })
    .expect("Error setting Ctrl-C handler");

    tokio::select! {
        server = server_fut => {
            match server {
                Ok(()) => {
                    tracing::info!("Server finished gracefully. Not cancelling, dependent tasks already terminate if they lose their input");
                }
                Err(e) => {
                    tracing::error!("Server error: {e:?}");
                    cancellation_token.cancel();
                }
            }
        }
        result = encoder_fut => {
            tracing::warn!("Encoder finished: {result:?}");
            cancellation_token.cancel();
        }
        result = decoder_fut => {
            tracing::warn!("Decoder finished: {result:?}");
            cancellation_token.cancel();
        }
    };

    Ok(())
}
