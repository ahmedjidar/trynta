//! Time-based one-time passwords, RFC 6238 (SPEC-V1 §4.1, §7.2, AC11).
//!
//! Nothing here is novel and nothing here should be: TOTP is HMAC over a counter
//! with a published truncation, and the only way to get it wrong is to deviate.
//! CLAUDE.md §4.1 — never invent cryptography — applies with full force to code
//! that *looks* simple enough to improvise.
//!
//! Three details that are easy to skip and change the answer:
//!
//! - **The counter is 8 bytes, big-endian, always.** Not the natural width of
//!   the platform's integer, and not truncated to fit a smaller buffer.
//! - **Dynamic truncation reads the low nibble of the *last* byte** of the MAC,
//!   which differs per algorithm because the MAC lengths differ. Hard-coding
//!   offset 19 works for SHA-1 and silently produces wrong codes for SHA-256 and
//!   SHA-512 — the exact failure AC11 exists to catch.
//! - **The high bit of the truncated word is masked off.** Skipping the mask
//!   gives the right answer roughly half the time, which is the worst possible
//!   frequency for a bug.
//!
//! The shared secret arrives as base32 and lives in a `Zeroizing` buffer from
//! the moment it is decoded to the moment the code is returned.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use zeroize::Zeroizing;

use crate::services::base32::{self, Base32Error};

/// Hash function backing the HMAC (SPEC-V1 §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Algorithm {
    /// The default every authenticator understands.
    #[default]
    Sha1,
    /// SHA-256.
    Sha256,
    /// SHA-512.
    Sha512,
}

impl Algorithm {
    /// Parse the `algorithm` parameter of an `otpauth://` URI.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_uppercase().as_str() {
            "SHA1" => Some(Self::Sha1),
            "SHA256" => Some(Self::Sha256),
            "SHA512" => Some(Self::Sha512),
            _ => None,
        }
    }

    /// The canonical name, for round-tripping a URI.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }
}

/// Default step, in seconds.
pub const DEFAULT_PERIOD: u32 = 30;
/// Default digit count.
pub const DEFAULT_DIGITS: u8 = 6;

/// Why a TOTP operation failed.
///
/// No variant carries the secret, the decoded seed, or a generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TotpError {
    /// The shared secret is not valid base32.
    #[error("the shared secret is not valid base32")]
    Secret(#[from] Base32Error),

    /// `digits` was not 6 or 8 (SPEC-V1 §4.1).
    #[error("a one-time code must be 6 or 8 digits")]
    Digits,

    /// `period` was zero, which has no meaning and would divide by zero.
    #[error("the time step must be at least one second")]
    Period,

    /// The URI was not a parseable `otpauth://totp/...`.
    #[error("that is not a valid otpauth:// URI")]
    Uri,

    /// The HMAC could not be constructed.
    ///
    /// Unreachable: HMAC accepts a key of any length by construction. Reported
    /// rather than aborted, unlike the equivalent branches in `keyring-crypto`
    /// (ADD-003), because the hazard is different. There, an uncomputed MAC
    /// would be *used* as though it were real. Here, failing closed means
    /// showing no code — and aborting the whole application because someone
    /// pasted an odd secret would be a denial of service on the user's own
    /// vault.
    #[error("the one-time code could not be computed")]
    Mac,
}

/// A parsed TOTP configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpConfig {
    /// Base32 shared secret, as the issuer gave it.
    pub secret: String,
    /// Hash function.
    pub algorithm: Algorithm,
    /// 6 or 8.
    pub digits: u8,
    /// Step, in seconds.
    pub period_seconds: u32,
    /// Issuer label, for display.
    pub issuer: String,
    /// Account label, for display.
    pub account: String,
}

impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            algorithm: Algorithm::default(),
            digits: DEFAULT_DIGITS,
            period_seconds: DEFAULT_PERIOD,
            issuer: String::new(),
            account: String::new(),
        }
    }
}

/// A code and how long it remains valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    /// The code, zero-padded to `digits`.
    pub value: String,
    /// Seconds until the step rolls over. Never zero: at the instant of
    /// rollover the *next* code has a full period ahead of it, and a UI showing
    /// "0s" for one tick reads as expired when it is not.
    pub seconds_remaining: u32,
    /// The step length, so the UI can size a countdown without a second call.
    pub period: u32,
}

/// Compute the code for `unix_seconds`.
///
/// Time is a parameter rather than read here, so the known-answer vectors can be
/// run at the exact instants RFC 6238 specifies.
///
/// # Errors
///
/// [`TotpError::Secret`] if the base32 does not decode, [`TotpError::Digits`] if
/// `digits` is not 6 or 8, [`TotpError::Period`] if `period_seconds` is zero.
pub fn code_at(config: &TotpConfig, unix_seconds: u64) -> Result<Code, TotpError> {
    if config.digits != 6 && config.digits != 8 {
        return Err(TotpError::Digits);
    }
    if config.period_seconds == 0 {
        return Err(TotpError::Period);
    }

    let key = base32::decode(&config.secret)?;
    let period = u64::from(config.period_seconds);
    let counter = unix_seconds / period;

    let digest = mac(config.algorithm, &key, counter.to_be_bytes())?;
    let value = truncate(&digest, config.digits);

    let elapsed = unix_seconds % period;
    let remaining = period - elapsed;

    Ok(Code {
        value,
        seconds_remaining: u32::try_from(remaining).unwrap_or(config.period_seconds),
        period: config.period_seconds,
    })
}

/// HMAC the counter under the requested hash.
///
/// Returns `Zeroizing`: the MAC is derived directly from the shared secret, and
/// leaking it would leak the ability to generate every future code.
///
/// # Errors
///
/// [`TotpError::Mac`], which cannot occur — HMAC accepts a key of any length.
fn mac(
    algorithm: Algorithm,
    key: &[u8],
    counter: [u8; 8],
) -> Result<Zeroizing<Vec<u8>>, TotpError> {
    fn finish<M: Mac>(mut m: M, counter: [u8; 8]) -> Zeroizing<Vec<u8>> {
        m.update(&counter);
        Zeroizing::new(m.finalize().into_bytes().to_vec())
    }

    Ok(match algorithm {
        Algorithm::Sha1 => finish(
            <Hmac<Sha1> as Mac>::new_from_slice(key).map_err(|_| TotpError::Mac)?,
            counter,
        ),
        Algorithm::Sha256 => finish(
            <Hmac<Sha256> as Mac>::new_from_slice(key).map_err(|_| TotpError::Mac)?,
            counter,
        ),
        Algorithm::Sha512 => finish(
            <Hmac<Sha512> as Mac>::new_from_slice(key).map_err(|_| TotpError::Mac)?,
            counter,
        ),
    })
}

/// RFC 4226 §5.3 dynamic truncation.
///
/// The offset comes from the low nibble of the **last** byte, which is why this
/// takes the digest by slice rather than assuming a length. SHA-1 gives 20
/// bytes, SHA-256 gives 32, SHA-512 gives 64; hard-coding 19 produces wrong
/// codes for two of the three.
fn truncate(digest: &[u8], digits: u8) -> String {
    let last = digest.last().copied().unwrap_or(0);
    let offset = usize::from(last & 0x0F);

    let mut word: u32 = 0;
    for i in 0..4 {
        word = (word << 8) | u32::from(digest.get(offset + i).copied().unwrap_or(0));
    }
    // Mask the sign bit: RFC 4226 works in signed 31-bit space so that
    // implementations in languages without unsigned integers agree.
    word &= 0x7FFF_FFFF;

    let modulus = 10u32.pow(u32::from(digits));
    format!("{:0width$}", word % modulus, width = usize::from(digits))
}

/// Parse an `otpauth://totp/...` URI (SPEC-V1 §4.1).
///
/// Hand-parsed rather than routed through a URL crate, because `otpauth` is a
/// non-special scheme and the part that matters — the query string — is a flat
/// list of percent-encoded pairs. What the label needs is percent-decoding, not
/// authority parsing, host normalisation or IDNA.
///
/// Non-default algorithms are honoured, which §4.1 calls out explicitly: an
/// implementation that ignores `algorithm=SHA256` produces plausible-looking
/// codes that never work.
///
/// # Errors
///
/// [`TotpError::Uri`] if the scheme or type is wrong or the secret is missing,
/// [`TotpError::Digits`] or [`TotpError::Period`] on out-of-range parameters.
pub fn parse_uri(uri: &str) -> Result<TotpConfig, TotpError> {
    let rest = uri
        .strip_prefix("otpauth://")
        .or_else(|| uri.strip_prefix("OTPAUTH://"))
        .ok_or(TotpError::Uri)?;

    let (kind, after_kind) = rest.split_once('/').ok_or(TotpError::Uri)?;
    if !kind.eq_ignore_ascii_case("totp") {
        // HOTP is counter-based and is not what SPEC-V1 §4.1 specifies. Refusing
        // is better than importing it as a TOTP config that never matches.
        return Err(TotpError::Uri);
    }

    let (label, query) = match after_kind.split_once('?') {
        Some((label, query)) => (label, query),
        None => (after_kind, ""),
    };

    let label = percent_decode(label);
    let (label_issuer, account) = match label.split_once(':') {
        Some((issuer, account)) => (issuer.trim().to_owned(), account.trim().to_owned()),
        None => (String::new(), label.trim().to_owned()),
    };

    let mut config = TotpConfig {
        issuer: label_issuer,
        account,
        ..TotpConfig::default()
    };

    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value);
        match key.to_ascii_lowercase().as_str() {
            "secret" => config.secret = value,
            // The query issuer wins over the label's: RFC-adjacent practice, and
            // it is the one the issuer set deliberately rather than the one that
            // fell out of a display string.
            "issuer" if !value.is_empty() => config.issuer = value,
            "algorithm" => config.algorithm = Algorithm::parse(&value).ok_or(TotpError::Uri)?,
            "digits" => {
                config.digits = value.parse::<u8>().map_err(|_| TotpError::Digits)?;
            }
            "period" => {
                config.period_seconds = value.parse::<u32>().map_err(|_| TotpError::Period)?;
            }
            _ => {}
        }
    }

    if config.secret.is_empty() {
        return Err(TotpError::Uri);
    }
    if config.digits != 6 && config.digits != 8 {
        return Err(TotpError::Digits);
    }
    if config.period_seconds == 0 {
        return Err(TotpError::Period);
    }
    // Decoded once at parse time so a bad secret is rejected while the user is
    // looking at the paste, not thirty seconds later when a code is wanted.
    let _ = base32::decode(&config.secret)?;

    Ok(config)
}

/// Percent-decode, treating `+` as a literal.
///
/// `+` means space in form encoding and nothing in a URI path, and `otpauth://`
/// labels are paths. Treating it as a space would corrupt account names that
/// legitimately contain one.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes.get(i).copied().unwrap_or(b'?');
        if byte == b'%' && i + 2 < bytes.len() {
            let hi = bytes.get(i + 1).copied().unwrap_or(b'?');
            let lo = bytes.get(i + 2).copied().unwrap_or(b'?');
            if let (Some(h), Some(l)) = (hex_value(hi), hex_value(lo)) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(byte);
        i += 1;
    }
    // Lossy: a label is a display string, and a malformed escape should render
    // as a replacement character rather than reject an otherwise valid config.
    String::from_utf8_lossy(&out).into_owned()
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
