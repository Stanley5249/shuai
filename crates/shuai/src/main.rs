use miette::{IntoDiagnostic, Result, WrapErr};
use shuai::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use url::Url;

const ENDPOINT: &str = "https://ws.generals.io";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .pretty()
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let endpoint = Url::parse(ENDPOINT).into_diagnostic()?;
    let user_id = std::env::var("GENERALS_IO_USER_ID")
        .into_diagnostic()
        .wrap_err("GENERALS_IO_USER_ID not set")?;

    let shutdown = CancellationToken::new();
    tokio::spawn(cancel_on_ctrl_c(shutdown.clone()));

    let mut session = Session::connect(endpoint, user_id, shutdown).await?;
    // A fixed room lets players rejoin each other. Otherwise every match gets a new room.
    if let Ok(room_id) = std::env::var("GENERALS_IO_ROOM") {
        session = session.with_mode(GameMode::FixedPrivate(room_id));
    }
    session.run(play_opening).await
}
