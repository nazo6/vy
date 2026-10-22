use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use vy_proto::{FrameDecoder, RpcMessage, decode_message, encode_frame, encode_message};

use crate::session::Session;

/// 標準入出力で RPC を処理します。
///
/// ログは必ず標準エラーへ出します。標準出力はフレーム専用です。
pub async fn serve() -> std::io::Result<()> {
    let mut input = tokio::io::stdin();
    let mut output = tokio::io::stdout();
    serve_with(&mut input, &mut output).await
}

/// 読み書きを差し替えられる形にした本体です。テストから使います。
pub async fn serve_with<R, W>(input: &mut R, output: &mut W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut decoder = FrameDecoder::new();
    let mut chunk = vec![0u8; 8 * 1024];
    let mut session = Session::new();

    loop {
        let read = input.read(&mut chunk).await?;

        if read == 0 {
            tracing::info!("標準入力が閉じました");
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
                tracing::info!("クライアントの要求で接続を閉じます");
                return Ok(());
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
