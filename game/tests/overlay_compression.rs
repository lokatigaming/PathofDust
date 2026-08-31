//! Overlay-bandwidth work phase 1 item 1 (2026-08-20) - opt-in, per-
//! client application-layer WS compression. This is the hard invariant
//! the whole feature was built around: a client that does NOT present
//! the exact `?wire=deflate` key must receive plain text frames, byte-
//! identical to today, and the server must never infer opt-in from
//! anything else. Old desktop clients fail SILENTLY on an unexpected
//! binary frame (`JSON.parse` of a `Blob` throws inside an empty catch -
//! the client just stops receiving fights, no error surfaced), so this
//! needs real end-to-end proof, not just a unit test of the shared
//! `send_ready_message` helper inside `game`'s own test suite. The exact
//! key/value (`wire=deflate`) is a hard contract with the already-
//! shipped PathOfDust_Desktop 2.6.0 client - not a free choice on this
//! side.
//!
//! Both connections are opened against the SAME running server instance,
//! one compressed and one not, deliberately overlapping - proving the
//! opt-in is truly per-connection (derived fresh from each connection's
//! own query string) and not some shared/global toggle that could leak
//! a compressed client's setting onto a plain one.

use futures_util::StreamExt;
use std::path::PathBuf;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn a_connection_without_wire_deflate_gets_text_only_even_while_a_compressed_client_is_also_connected() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("overlay_compression_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    let characters_path = scratch.join("adventure-characters.json");
    std::fs::write(&characters_path, "{}").expect("failed to seed an empty characters file");
    let sessions_path = scratch.join("adventure-sessions.json");

    let manager = game::adventure::AdventureManager::new(characters_path, PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound_addr = game::adventure_web::start_adventure_web_server(
        0,
        "http://localhost".to_string(),
        Some("test-client-id".to_string()),
        Some("test-client-secret".to_string()),
        manager.clone(),
        sessions_path,
        None,
    )
    .await
    .expect("disposable adventure_web server must start");
    let port = bound_addr.port();

    // Compressed client connects first and stays open - if opt-in were
    // ever accidentally global/shared state, this is what would leak.
    let (mut compressed, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws?wire=deflate")).await.expect("compressed client failed to connect");
    let compressed_first = compressed.next().await.expect("compressed client got no message at all").expect("compressed client's first message was a transport error");
    let Message::Binary(compressed_bytes) = compressed_first else {
        panic!("a client that DID present ?wire=deflate must receive a Binary frame, got {compressed_first:?}");
    };
    assert_eq!(compressed_bytes[0], 0x78, "the first compressed frame must begin with the zlib header byte (0x78) - a regression back to raw deflate must fail THIS assertion");
    let mut decoder = flate2::read::ZlibDecoder::new(&compressed_bytes[..]);
    let mut decompressed = String::new();
    std::io::Read::read_to_string(&mut decoder, &mut decompressed).expect("compressed frame must be valid zlib (RFC 1950), matching DecompressionStream('deflate') on both the OBS/direct-viewer client and PathOfDust_Desktop 2.6.0");
    assert!(decompressed.contains("\"type\":\"state\""), "decompressed payload must be the real state envelope, got: {decompressed}");

    // Plain client connects while the compressed one is still open.
    let (mut plain, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws")).await.expect("plain client failed to connect");
    let plain_first = plain.next().await.expect("plain client got no message at all").expect("plain client's first message was a transport error");
    match plain_first {
        Message::Text(text) => assert!(text.contains("\"type\":\"state\""), "plain client's text frame must be the real state envelope, got: {text}"),
        other => panic!("a client that did NOT present ?wire=deflate must receive a Text frame - byte-identical behavior to before this feature existed - got {other:?} instead"),
    }

    // A second plain connection, with variant/wrong values - none of
    // these should ever be treated as opt-in.
    for bad_value in ["1", "true", "gzip", "DEFLATE"] {
        let (mut variant, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws?wire={bad_value}")).await.expect("variant client failed to connect");
        let variant_first = variant.next().await.expect("variant client got no message at all").expect("variant client's first message was a transport error");
        assert!(matches!(variant_first, Message::Text(_)), "wire={bad_value} (not exactly \"deflate\") must still be treated as no opt-in - got {variant_first:?}");
    }
}
