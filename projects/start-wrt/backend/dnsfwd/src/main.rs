use std::io::IsTerminal;
use std::process::ExitCode;

use dnsfwd::config::Config;
use dnsfwd::{serve, Forwarder};
use tokio::net::{TcpListener, UdpSocket};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .without_time()
        .init();

    let mut args = std::env::args().skip(1);
    let (Some(path), None) = (args.next(), args.next()) else {
        eprintln!("usage: startwrt-dnsfwd <config.json>");
        return ExitCode::from(2);
    };
    let config = match std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|json| Config::parse(&json))
    {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async {
        let forwarder = Forwarder::new(&config).map_err(|e| e.to_string())?;
        let udp = UdpSocket::bind(config.listen)
            .await
            .map_err(|e| e.to_string())?;
        let tcp = TcpListener::bind(config.listen)
            .await
            .map_err(|e| e.to_string())?;
        serve(forwarder, udp, tcp).await.map_err(|e| e.to_string())
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}: {e}", config.listen);
            ExitCode::FAILURE
        }
    }
}
