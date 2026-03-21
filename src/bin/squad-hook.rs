use anyhow::{bail, Context, Result};
use squad::daemon::{send_request, DaemonPaths};
use squad::protocol::Request;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());
    let workspace = std::env::current_dir()?;

    match command.as_str() {
        "send" => {
            let to = args.next().context("send requires <to> <message>")?;
            let content = args.next().context("send requires <to> <message>")?;
            let from = std::env::var("SQUAD_HOOK_FROM").unwrap_or_else(|_| "hook".to_string());
            let paths = DaemonPaths::new(&workspace);
            let response = send_request(
                paths.socket_path(),
                &Request::SendMessage { from, to, content },
            )
            .await?;
            match response {
                squad::protocol::Response::Ok(_) => Ok(()),
                squad::protocol::Response::Error { message } => bail!(message),
            }
        }
        "help" | "--help" | "-h" => {
            println!("Usage: squad-hook send <to> <message>");
            Ok(())
        }
        other => bail!("unknown command: {other}"),
    }
}
