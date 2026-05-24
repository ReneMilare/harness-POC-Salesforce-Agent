use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use vps_rust::{AppState, Config, app, discord};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vps_rust=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();
    let addr = config
        .socket_addr()
        .unwrap_or_else(|err| panic!("configuração de bind inválida: {err}"));

    tracing::info!(%addr, "iniciando servidor");
    run(addr, config).await;
}

async fn run(addr: SocketAddr, config: Config) {
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| panic!("falha ao abrir porta {addr}: {err}"));
    let state = AppState::new(config);

    if state.config.discord_bot_token.is_some() {
        let discord_state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = discord::run(discord_state).await {
                tracing::error!(%error, "bot Discord finalizado com erro");
            }
        });
    }

    axum::serve(listener, app(state))
        .await
        .expect("servidor axum falhou");
}
