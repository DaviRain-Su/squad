use crate::mcp::McpServer;
use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct StdioTransport<R, W> {
    reader: R,
    writer: W,
    server: McpServer,
}

impl<R, W> StdioTransport<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn new(reader: R, writer: W, server: McpServer) -> Self {
        Self {
            reader,
            writer,
            server,
        }
    }

    pub async fn serve(&mut self) -> Result<()> {
        loop {
            match self.serve_once().await {
                Ok(()) => {}
                Err(error) if is_eof(&error) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    }

    pub async fn serve_once(&mut self) -> Result<()> {
        let request = self.read_message().await?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let response = match self.server.handle_request(request).await {
            Ok(response) => response,
            Err(error) => McpServer::error_response(id, &error.to_string()),
        };
        self.write_message(&response).await
    }

    async fn read_message(&mut self) -> Result<Value> {
        let content_length = self.read_content_length().await?;
        let mut body = vec![0_u8; content_length];
        self.reader.read_exact(&mut body).await?;
        serde_json::from_slice(&body).context("failed to parse JSON-RPC body")
    }

    async fn read_content_length(&mut self) -> Result<usize> {
        let mut header = Vec::new();
        let mut byte = [0_u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            self.reader.read_exact(&mut byte).await?;
            header.push(byte[0]);
        }

        let header = String::from_utf8(header).context("request header is not valid UTF-8")?;
        header
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .context("missing Content-Length header")?
            .trim()
            .parse::<usize>()
            .context("invalid Content-Length header")
    }

    async fn write_message(&mut self, response: &Value) -> Result<()> {
        let body = serde_json::to_vec(response)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.writer.write_all(header.as_bytes()).await?;
        self.writer.write_all(&body).await?;
        self.writer.flush().await?;
        Ok(())
    }
}

fn is_eof(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|source| source.downcast_ref::<std::io::Error>())
        .any(|io_error| io_error.kind() == std::io::ErrorKind::UnexpectedEof)
}
