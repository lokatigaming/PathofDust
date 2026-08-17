// One-time setup binary — ports get-patreon-token.js. Run once (and again
// if you ever need to re-authorize). Walks you through Patreon's OAuth
// login in your browser, then saves a refreshable token to
// patreon-tokens.json.

use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::sync::oneshot;
use twitch_bot_rs::patreon::PatreonTokens;

const REDIRECT_URI: &str = "http://localhost:3001/callback";
const PORT: u16 = 3001;
const SCOPES: &[&str] = &["campaigns", "campaigns.members"];

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

#[derive(Clone)]
struct AppState {
    client_id: String,
    client_secret: String,
    shutdown_tx: std::sync::Arc<std::sync::Mutex<Option<oneshot::Sender<()>>>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let client_id = std::env::var("PATREON_CLIENT_ID")
        .map_err(|_| anyhow::anyhow!("Missing PATREON_CLIENT_ID in .env"))?;
    let client_secret = std::env::var("PATREON_CLIENT_SECRET")
        .map_err(|_| anyhow::anyhow!("Missing PATREON_CLIENT_SECRET in .env"))?;

    let mut authorize_url = url::Url::parse("https://www.patreon.com/oauth2/authorize")?;
    authorize_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", &SCOPES.join(" "));

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let shutdown_tx = std::sync::Arc::new(std::sync::Mutex::new(Some(shutdown_tx)));
    let state = AppState { client_id, client_secret, shutdown_tx };

    let app = axum::Router::new()
        .route("/callback", get(callback))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await?;

    println!(
        "Opening browser for Patreon login...\nIf it doesn't open automatically, visit:\n{}\n",
        authorize_url
    );
    let _ = open::that(authorize_url.as_str());

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        })
        .await?;

    Ok(())
}

async fn callback(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Html<String> {
    let result = handle_callback(&state, &params).await;

    if let Some(tx) = state.shutdown_tx.lock().unwrap().take() {
        let _ = tx.send(());
    }

    match result {
        Ok(()) => {
            println!("\nSaved patreon-tokens.json — the bot will pick this up on next start.\n");
            Html("<h1>Authorized.</h1><p>patreon-tokens.json has been saved. You can close this tab.</p>".to_string())
        }
        Err(err) => {
            eprintln!("{err}");
            Html(format!("<h1>Something went wrong.</h1><p>{err}</p>"))
        }
    }
}

async fn handle_callback(state: &AppState, params: &HashMap<String, String>) -> anyhow::Result<()> {
    if let Some(error) = params.get("error") {
        anyhow::bail!("Authorization failed: {error}");
    }
    let code = params
        .get("code")
        .ok_or_else(|| anyhow::anyhow!("No code returned."))?;

    let http = reqwest::Client::new();
    let resp = http
        .post("https://www.patreon.com/api/oauth2/token")
        .form(&[
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
            ("client_id", state.client_id.as_str()),
            ("client_secret", state.client_secret.as_str()),
            ("redirect_uri", REDIRECT_URI),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Patreon token exchange failed: {body}");
    }

    let data: TokenResponse = resp.json().await?;
    let tokens = PatreonTokens {
        access_token: data.access_token,
        refresh_token: data.refresh_token,
        expires_in: data.expires_in,
        obtainment_timestamp: chrono::Utc::now().timestamp_millis() as u64,
        campaign_id: None,
        campaign_url: None,
    };

    twitch_bot_rs::state::save_json("patreon-tokens.json", &tokens)?;
    Ok(())
}
