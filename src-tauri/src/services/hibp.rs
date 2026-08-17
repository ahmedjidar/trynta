//! The HIBP range transport — one of exactly three outbound requests (CLAUDE.md §4.7).
//!
//! Everything privacy-relevant about a breach check is decided in
//! [`crate::services::breach`]: the SHA-1 split, which five characters may leave
//! the machine, and how a response is parsed. This module is only the pipe, and it
//! is a separate file so the pipe can be read in one sitting and audited against
//! SPEC-V1 §7.4.
//!
//! What leaves the machine, exhaustively:
//!
//! | | |
//! |---|---|
//! | Method and path | `GET /range/<5 hex chars>` |
//! | Host | `api.pwnedpasswords.com`, hardcoded, HTTPS only, no redirects |
//! | `Add-Padding` | `true` — **mandatory**, see below |
//! | `User-Agent` | `Keyring`, with no version |
//! | Anything else | nothing. No cookie, no token, no account, no vault id |
//!
//! **`Add-Padding` is not optional.** SPEC-V1 §7.4: *"without it, response length
//! reveals which prefix you queried."* The 5-character prefix is meant to place the
//! password in a bucket of hundreds; a distinguishable response size collapses that
//! bucket back to one, and the k-anonymity is gone. So it is set here, once, on a
//! request builder no caller can construct without it, rather than being passed in
//! as a flag someone could forget.
//!
//! **Redirects are refused.** A redirect is a server-controlled instruction to send
//! the same path to a different host, and the path is the thing we are protecting.
//! `max_redirects(0)` with `max_redirects_will_error(true)` turns that into an error
//! rather than a silent forward.
//!
//! **The version is deliberately absent from the `User-Agent`.** HIBP asks API
//! consumers to identify themselves, and `Keyring` does that. Appending a version
//! would split users into cohorts and make a single install more distinguishable
//! across queries, for no benefit to anyone.

use std::time::Duration;

use crate::services::breach::{BreachError, Prefix, RangeSource};

/// The only host this module will talk to.
pub const HOST: &str = "api.pwnedpasswords.com";

/// The range endpoint, prefix appended.
const ENDPOINT: &str = "https://api.pwnedpasswords.com/range/";

/// What we tell HIBP we are. No version — see the module docs.
const USER_AGENT: &str = "Keyring";

/// Ceiling on one range response.
///
/// A padded range response is tens of kilobytes. 1 MiB is far above anything real
/// and far below anything that could exhaust memory, and a response that exceeds it
/// is not a range body.
const MAX_BODY: u64 = 1024 * 1024;

/// Whole-request budget, connect through last byte.
const TIMEOUT: Duration = Duration::from_secs(15);

/// The live HIBP range client.
///
/// Holds a connection pool, so a check over many prefixes reuses one TLS session
/// instead of renegotiating per password.
pub struct HibpClient {
    /// Configured once in [`HibpClient::new`]; nothing mutates it afterwards.
    agent: ureq::Agent,
}

impl std::fmt::Debug for HibpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No secrets here, but a pool's Debug prints connection state and there is
        // no reason for any of it to reach a log.
        f.debug_struct("HibpClient").finish_non_exhaustive()
    }
}

impl Default for HibpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HibpClient {
    /// Build the client.
    #[must_use]
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            // TLS or nothing. A plaintext range query would put the prefix on the
            // wire for any observer, which is most of what k-anonymity defends.
            .https_only(true)
            .max_redirects(0)
            .max_redirects_will_error(true)
            .user_agent(USER_AGENT)
            .timeout_global(Some(TIMEOUT))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl RangeSource for HibpClient {
    fn fetch(&self, prefix: Prefix) -> Result<String, BreachError> {
        // `Prefix` is a validated newtype over five hex characters with no wider
        // constructor, so this URL cannot be steered anywhere by its contents.
        let url = format!("{ENDPOINT}{}", prefix.as_str());

        let mut response = self
            .agent
            .get(&url)
            .header("Add-Padding", "true")
            .call()
            .map_err(|_| BreachError::Unreachable)?;

        if response.status() != 200 {
            return Err(BreachError::Unreachable);
        }

        response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()
            .map_err(|_| BreachError::Unreachable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoint_is_https_and_the_expected_host() {
        assert!(ENDPOINT.starts_with("https://"));
        assert!(ENDPOINT.starts_with(&format!("https://{HOST}/")));
        assert!(ENDPOINT.ends_with("/range/"));
    }

    #[test]
    fn the_user_agent_carries_no_version() {
        assert_eq!(USER_AGENT, "Keyring");
        assert!(
            !USER_AGENT.contains(env!("CARGO_PKG_VERSION")),
            "a version in the UA splits installs into distinguishable cohorts"
        );
    }

    /// The URL is built from a `Prefix`, and a `Prefix` is five hex characters.
    ///
    /// This is the assertion that matters for the transport: whatever a caller
    /// does, the request path cannot be made to carry a suffix, a whole hash, or a
    /// query string.
    #[test]
    fn a_prefix_cannot_escape_the_path() {
        let (prefix, suffix) = crate::services::breach::split("correct horse battery");
        let url = format!("{ENDPOINT}{}", prefix.as_str());

        assert_eq!(url.len(), ENDPOINT.len() + 5);
        assert!(!url.contains(suffix.as_str()));
        assert!(!url.contains('?') && !url.contains('&') && !url.contains('#'));
        assert!(prefix
            .as_str()
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)));
    }
}
