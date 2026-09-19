// Strips credentials out of anything on its way to a log line.
//
// 2026-09-19, live on stream: three chat lines read `YouTube lookup
// failed: error sending request for url (https://www.googleapis.com/
// youtube/v3/search?…&key=<the key>)`. A `reqwest::Error`'s own Display
// prints the full request URL, the YouTube URL carries the API key in
// its query string, and `RequestError::Api` rendered that error straight
// into a chat reply. The key was broadcast to a live channel.
//
// TWO LAYERS, AND THIS IS THE SECOND. The first is that no error type
// whose Display can reach chat interpolates an underlying error at all —
// see `RequestError` in song_requests.rs, where the Display is a fixed
// sentence, so a new call site cannot reintroduce the leak by formatting
// the wrong thing. This layer is for the LOG, which is allowed to keep
// the path and query because they are what makes a failure diagnosable —
// with every sensitive value replaced by a fixed placeholder. Never a
// prefix of the value, never a length hint, never the value.

/// Query parameter names whose value is a credential. Matched
/// case-insensitively, and only at a real parameter position (start of
/// the string, or straight after `?` or `&`) so a parameter that merely
/// ends in one of these names — `monkey=`, `subtoken=` — is left alone.
///
/// The four that exist in this bot today are `key` (YouTube,
/// song_requests.rs), `api_key` (Last.fm, playrandom.rs) and `secret`
/// (the Apps Script sync, in personal_playlists.rs, essence_pricing.rs
/// and vessel_pricing.rs). The rest are here because the cost of
/// carrying a name that never appears is nothing, and the cost of
/// missing one that turns up later is a credential in a log.
const SENSITIVE_PARAMS: [&str; 11] = [
    "key",
    "api_key",
    "apikey",
    "secret",
    "client_secret",
    "token",
    "access_token",
    "refresh_token",
    "auth",
    "password",
    "signature",
];

/// Fixed, and deliberately says nothing about what it replaced.
pub const PLACEHOLDER: &str = "REDACTED";

/// True for the characters that end a query value inside a formatted
/// error. `&` is the real URL delimiter; the rest are how an error
/// message wraps a URL — `reqwest` writes `for url (https://…)`, so the
/// closing paren has to end the value or the placeholder would swallow
/// the rest of the line.
fn ends_value(byte: u8) -> bool {
    matches!(byte, b'&' | b')' | b'"' | b'\'' | b' ' | b'\t' | b'\n' | b'\r' | b',' | b'#')
}

/// If `rest` begins with `<sensitive>=`, the length of that `name=`
/// prefix.
fn sensitive_param_prefix(rest: &str) -> Option<usize> {
    SENSITIVE_PARAMS
        .iter()
        .filter(|name| {
            let len = name.len();
            rest.len() > len && rest.as_bytes()[len] == b'=' && rest[..len].eq_ignore_ascii_case(name)
        })
        // The match is anchored at a parameter boundary, so `api_key=`
        // can never be read as a bare `key=` — `max` is only here so
        // that if two names ever did overlap, the longer one wins.
        .map(|name| name.len() + 1)
        .max()
}

/// Returns `text` with the value of every sensitive query parameter
/// replaced by `PLACEHOLDER`. Everything else — the host, the path, the
/// harmless parameters, the surrounding prose — is left exactly as it
/// was, because that is the part that makes a log line worth having.
pub fn redact(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    while i < bytes.len() {
        let at_param_start = i == 0 || bytes[i - 1] == b'?' || bytes[i - 1] == b'&';
        if at_param_start {
            if let Some(prefix) = sensitive_param_prefix(&text[i..]) {
                out.push_str(&text[i..i + prefix]);
                out.push_str(PLACEHOLDER);
                i += prefix;
                while i < bytes.len() && !ends_value(bytes[i]) {
                    i += 1;
                }
                continue;
            }
        }
        // One whole char at a time — `i` must stay on a char boundary,
        // and an error message can carry any UTF-8 at all.
        let ch = text[i..].chars().next().expect("i is on a char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live line, verbatim in shape, with a stand-in value. The
    /// path and the harmless parameters survive — that is the point of
    /// redacting rather than dropping the URL — and the value does not
    /// appear in any form.
    #[test]
    fn the_log_line_for_a_failed_lookup_keeps_the_query_but_not_the_key() {
        let value = "AIzaSyEXAMPLEEXAMPLEEXAMPLEEXAMPLE";
        let line = format!(
            "error sending request for url (https://www.googleapis.com/youtube/v3/search?part=snippet&type=video&q=trapt+echo&key={value})"
        );

        let redacted = redact(&line);

        assert!(!redacted.contains(value), "the value is gone");
        assert!(!redacted.contains(&value[..4]), "and so is any prefix of it");
        assert_eq!(
            redacted,
            "error sending request for url (https://www.googleapis.com/youtube/v3/search?part=snippet&type=video&q=trapt+echo&key=REDACTED)"
        );
    }

    /// A key in the middle of a query, not at its end — the value stops
    /// at the `&`, and what follows is untouched.
    #[test]
    fn a_key_in_the_middle_of_a_query_stops_at_the_ampersand() {
        assert_eq!(
            redact("https://host/p?key=SEKRIT&part=snippet"),
            "https://host/p?key=REDACTED&part=snippet"
        );
    }

    /// Every credential this bot actually puts in a query string.
    #[test]
    fn it_redacts_every_parameter_the_bot_really_uses() {
        assert_eq!(
            redact("https://ws.audioscrobbler.com/2.0/?method=artist.gettoptags&api_key=LASTFMVALUE&format=json"),
            "https://ws.audioscrobbler.com/2.0/?method=artist.gettoptags&api_key=REDACTED&format=json"
        );
        assert_eq!(
            redact("https://script.google.com/exec?action=syncEssencePricing&secret=SHEETVALUE"),
            "https://script.google.com/exec?action=syncEssencePricing&secret=REDACTED"
        );
    }

    /// A parameter that merely ENDS in a sensitive name is not one, and
    /// neither is the word `key` in prose. Over-redacting a log is a
    /// smaller harm than under-redacting it, but it is still a harm —
    /// the log has to stay readable.
    #[test]
    fn it_leaves_alone_what_only_looks_sensitive() {
        assert_eq!(redact("https://host/p?monkey=banana"), "https://host/p?monkey=banana");
        assert_eq!(redact("the key was rotated"), "the key was rotated");
        assert_eq!(redact("PayPal relay poll failed: error sending request for url (https://relay/pending-tips)"), "PayPal relay poll failed: error sending request for url (https://relay/pending-tips)");
    }

    /// An error message is arbitrary text, not necessarily a URL and not
    /// necessarily ASCII — redacting must never panic or corrupt it.
    #[test]
    fn it_survives_text_that_is_not_a_url() {
        assert_eq!(redact(""), "");
        assert_eq!(redact("YouTube won't play that — “blocked”, naïvely"), "YouTube won't play that — “blocked”, naïvely");
        // A trailing `key=` with no value at all.
        assert_eq!(redact("https://host/p?key="), "https://host/p?key=REDACTED");
    }
}
