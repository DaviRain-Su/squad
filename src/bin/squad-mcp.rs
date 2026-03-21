use anyhow::Result;
use squad::mcp::transport::StdioTransport;
use squad::mcp::McpServer;

#[tokio::main]
async fn main() -> Result<()> {
    let server = McpServer::from_cwd()?;
    let mut transport = StdioTransport::new(tokio::io::stdin(), tokio::io::stdout(), server);
    transport.serve().await
}
