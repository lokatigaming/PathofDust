// One-time setup binary — ports get-token.js. Run this once (and again if
// you ever need to re-authorize with different scopes). Walks you through
// Twitch's OAuth login in your browser, then saves a refreshable token to
// tokens.json. The main bot never needs this binary again once tokens.json
// exists (if you already have one from the Node bot, you can just copy it
// here instead of running this at all — same JSON shape).

use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::sync::oneshot;
use twitch_bot_rs::twitch::auth::TwitchTokens;

const REDIRECT_URI: &str = "http://localhost:3000/callback";
const PORT: u16 = 3000;

const SCOPES: &[&str] = &[
    "chat:read",
    "chat:edit",
    "moderator:read:followers",
    "channel:read:subscriptions",
    "bits:read",
    // Create/manage the self-service "Set Entrance Theme Song" channel
    // points reward, and fulfill/refund its redemptions.
    "channel:manage:redemptions",
];

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    scope: Vec<String>,
    expires_in: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let client_id = std::env::var("TWITCH_CLIENT_ID")
        .map_err(|_| anyhow::anyhow!("Missing TWITCH_CLIENT_ID in .env"))?;
    let client_secret = std::env::var("TWITCH_CLIENT_SECRET")
        .map_err(|_| anyhow::anyhow!("Missing TWITCH_CLIENT_SECRET in .env"))?;

    let mut authorize_url = url::Url::parse("https://id.twitch.tv/oauth2/authorize")?;
    authorize_url
        .query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("response_type", "code")
        .append_pair("scope", &SCOPES.join(" "));

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let shutdown_tx = std::sync::Arc::new(std::sync::Mutex::new(Some(shutdown_tx)));

    let state = AppState {
        client_id,
        client_secret,
        shutdown_tx,
    };

    let app = axum::Router::new()
        .route("/callback", get(callback))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await?;

    println!(
        "Opening browser for Twitch login...\nIf it doesn't open automatically, visit:\n{}\n",
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

#[derive(Clone)]
struct AppState {
    client_id: String,
    client_secret: String,
    shutdown_tx: std::sync::Arc<std::sync::Mutex<Option<oneshot::Sender<()>>>>,
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
            println!("\nSaved tokens.json — you can now run: cargo run\n");
            Html("<h1>Authorized.</h1><p>tokens.json has been saved. You can close this tab and go back to the terminal.</p>".to_string())
        }
        Err(err) => {
            eprintln!("{err}");
            Html(format!("<h1>Something went wrong.</h1><p>{err}</p>"))
        }
    }
}

async fn handle_callback(state: &AppState, params: &HashMap<String, String>) -> anyhow::Result<()> {
    if let Some(error) = params.get("error_description") {
        anyhow::bail!("Authorization failed: {error}");
    }
    let code = params
        .get("code")
        .ok_or_else(|| anyhow::anyhow!("No code returned."))?;

    let http = reqwest::Client::new();
    let resp = http
        .post("https://id.twitch.tv/oauth2/token")
        .form(&[
            ("client_id", state.client_id.as_str()),
            ("client_secret", state.client_secret.as_str()),
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", REDIRECT_URI),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Twitch token exchange failed: {body}");
    }

    let data: TokenResponse = resp.json().await?;
    let tokens = TwitchTokens {
        access_token: data.access_token,
        refresh_token: data.refresh_token,
        scope: data.scope,
        expires_in: data.expires_in,
        obtainment_timestamp: chrono::Utc::now().timestamp_millis() as u64,
    };

    twitch_bot_rs::state::save_json("tokens.json", &tokens)?;
    Ok(())
}
