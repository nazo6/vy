use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use vy_proto::{FrameDecoder, RpcMessage, decode_message, encode_frame, encode_message};

use crate::session::Session;

/// Handles RPC over stdio.
///
/// Logs always go to stderr. Stdout is reserved for frames.
pub async fn serve() -> std::io::Result<()> {
    let mut input = tokio::io::stdin();
    let mut output = tokio::io::stdout();
    serve_with(&mut input, &mut output).await
}

/// Core loop with swappable I/O. Used by tests.
pub async fn serve_with<R, W>(input: &mut R, output: &mut W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut decoder = FrameDecoder::new();
    let mut chunk = vec![0u8; 8 * 1024];
    let (outgoing_tx, mut outgoing_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut session = Session::new();
    session.set_outgoing(outgoing_tx);

    loop {
        tokio::select! {
            Some(msg) = outgoing_rx.recv() => {
                write_message(output, &msg).await?;
            }
            res = input.read(&mut chunk) => {
                let read = res?;
                if read == 0 {
                    tracing::info!("stdin closed");
                    return Ok(());
                }

                decoder.push(&chunk[..read]);

                while let Some(payload) = decoder.next_frame().map_err(invalid_data)? {
                    let message = decode_message(&payload).map_err(invalid_data)?;
                    let outcome = session.handle(message);

                    for message in outcome.messages {
                        write_message(output, &message).await?;
                    }

                    if outcome.close {
                        tracing::info!("closing the connection at the client's request");
                        return Ok(());
                    }
                }
            }
        }
    }
}

async fn write_message<W>(writer: &mut W, message: &RpcMessage) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let payload = encode_message(message).map_err(invalid_data)?;
    let frame = encode_frame(&payload).map_err(invalid_data)?;
    writer.write_all(&frame).await?;
    writer.flush().await
}

fn invalid_data(error: vy_proto::ProtoError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
}
