// Outbound email (item 47b, 2026-10-02) - optional player emails and
// email password recovery, see accounts.rs.
//
// This partly reverses the 2026-09-02 "the game becomes fully standalone"
// decision (docs/external_integration_removal_scope.md): the game now
// talks to ONE outside service, an SMTP server of the owner's choosing,
// and only when it is configured. With no SMTP configuration `from_env`
// returns `None`, every email page 404s, the login page reads exactly as
// it did in stage 1, and nothing here is ever reached.
//
// Configuration comes from the environment / `.env` (main.rs loads it):
//   SMTP_HOST, SMTP_PORT (default 587; 465 means implicit TLS, anything
//   else STARTTLS), SMTP_USERNAME, SMTP_PASSWORD, SMTP_FROM (an address,
//   or `Name <address>`), PUBLIC_BASE_URL (e.g. https://adventure.lokati.net,
//   used to build the links in the mail). All but SMTP_PORT are required.
//
// Addresses are personal data: they are stored only in
// `adventure-accounts.json` and are never logged in full - every log line
// goes through `mask_email`.

use std::sync::Arc;

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, Message, SmtpTransport, Transport};

/// One email to send. Plain text only.
pub struct OutgoingMail {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// The transport seam. Production uses SMTP; the integration tests hand
/// `start_adventure_web_server_with_email` a recording fake, so the suite
/// sends no real email. Called on the blocking pool, never the reactor.
pub trait Mailer: Send + Sync {
    fn send(&self, mail: &OutgoingMail) -> anyhow::Result<()>;
}

/// Everything the email features need. `None` in `AppState` means
/// "SMTP not configured": every email feature is hidden and inert.
#[derive(Clone)]
pub struct EmailConfig {
    pub mailer: Arc<dyn Mailer>,
    /// The site's public base URL, no trailing slash.
    pub base_url: String,
}

impl EmailConfig {
    pub fn new(mailer: Arc<dyn Mailer>, base_url: &str) -> Self {
        Self { mailer, base_url: base_url.trim().trim_end_matches('/').to_string() }
    }
}

struct SmtpMailer {
    transport: SmtpTransport,
    from: Mailbox,
}

impl Mailer for SmtpMailer {
    fn send(&self, mail: &OutgoingMail) -> anyhow::Result<()> {
        let to: Mailbox = mail.to.parse()?;
        let message = Message::builder().from(self.from.clone()).to(to).subject(mail.subject.clone()).body(mail.body.clone())?;
        self.transport.send(&message)?;
        Ok(())
    }
}

const REQUIRED_VARS: [&str; 5] = ["SMTP_HOST", "SMTP_USERNAME", "SMTP_PASSWORD", "SMTP_FROM", "PUBLIC_BASE_URL"];

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Builds the SMTP mailer from the environment, or `None` when it is not
/// (fully) configured. Logs which variable NAMES are missing - never a
/// value.
pub fn from_env() -> Option<EmailConfig> {
    let missing: Vec<&str> = REQUIRED_VARS.iter().copied().filter(|name| env_value(name).is_none()).collect();
    if missing.len() == REQUIRED_VARS.len() {
        tracing::info!("Email features disabled: SMTP is not configured.");
        return None;
    }
    if !missing.is_empty() {
        tracing::warn!("Email features disabled: SMTP is partly configured; missing {}.", missing.join(", "));
        return None;
    }
    let value = |name: &str| env_value(name).unwrap_or_default();
    let Ok(from) = value("SMTP_FROM").parse::<Mailbox>() else {
        tracing::warn!("Email features disabled: SMTP_FROM is not a valid address.");
        return None;
    };
    let port = match env_value("SMTP_PORT").map(|p| p.parse::<u16>()) {
        None => 587,
        Some(Ok(port)) => port,
        Some(Err(_)) => {
            tracing::warn!("Email features disabled: SMTP_PORT is not a number.");
            return None;
        }
    };
    let host = value("SMTP_HOST");
    // 465 is implicit TLS ("SMTPS"); 587 and everything else upgrade with
    // STARTTLS. Both refuse to send over an unencrypted connection.
    let builder = if port == 465 { SmtpTransport::relay(&host) } else { SmtpTransport::starttls_relay(&host) };
    let transport = match builder {
        Ok(b) => b.port(port).credentials(Credentials::new(value("SMTP_USERNAME"), value("SMTP_PASSWORD"))).timeout(Some(std::time::Duration::from_secs(20))).build(),
        Err(err) => {
            tracing::warn!("Email features disabled: SMTP transport for the configured host could not be built: {err}");
            return None;
        }
    };
    tracing::info!("Email features enabled (SMTP port {port}).");
    Some(EmailConfig::new(Arc::new(SmtpMailer { transport, from }), &value("PUBLIC_BASE_URL")))
}

/// An address the game will accept: parses as an RFC 5321 address, has a
/// dotted domain, and fits in 254 bytes.
pub(super) fn valid_email(addr: &str) -> bool {
    addr.len() <= 254 && addr.parse::<Address>().is_ok_and(|a| a.domain().contains('.'))
}

/// `alice@example.com` -> `a***@e***.com`. The only form an address may
/// take in a log line.
pub(super) fn mask_email(addr: &str) -> String {
    let Some((local, domain)) = addr.rsplit_once('@') else {
        return "***".to_string();
    };
    let first = |s: &str| s.chars().next().map(String::from).unwrap_or_default();
    let (host, tld) = domain.rsplit_once('.').unwrap_or((domain, ""));
    let tld = if tld.is_empty() { String::new() } else { format!(".{tld}") };
    format!("{}***@{}***{tld}", first(local), first(host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_masked_in_logs() {
        assert_eq!(mask_email("alice@example.com"), "a***@e***.com");
        assert_eq!(mask_email("bob@mail.example.co.uk"), "b***@m***.uk");
        assert_eq!(mask_email("not-an-address"), "***");
        assert!(!mask_email("alice@example.com").contains("alice"));
    }

    #[test]
    fn only_plausible_addresses_are_accepted() {
        assert!(valid_email("player@example.com"));
        assert!(!valid_email("player@localhost"));
        assert!(!valid_email("no at sign"));
        assert!(!valid_email("two@@example.com"));
        assert!(!valid_email(&format!("{}@example.com", "a".repeat(250))));
    }
}
