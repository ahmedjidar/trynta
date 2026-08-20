// SPDX-License-Identifier: AGPL-3.0-or-later
//! Windows Hello, via `KeyCredentialManager` (SPEC-V1 §5.1, §8).
//!
//! `KeyCredentialManager` creates an asymmetric key that the OS gates behind a
//! Hello check and stores in the TPM where one exists. We never see the private
//! key; we ask it to sign a fixed challenge, which prompts the user, and derive
//! a wrapping key from the signature.
//!
//! Two properties make that sound:
//!
//! - **The signature is deterministic.** `KeyCredential` signs with RSA
//!   PKCS#1 v1.5, which has no random nonce, so the same challenge yields the
//!   same bytes every time — that is what makes it usable as key material
//!   rather than merely as a yes/no answer.
//! - **Enrolment changes destroy the key.** Windows deletes the credential when
//!   Hello credentials change, so `OpenAsync` then fails and we fall back to the
//!   master password. SPEC-V1 §5.1 says to rely on that rather than track
//!   enrolment ourselves, and we do.
//!
//! The signature is run through HKDF before use rather than used directly: it is
//! a long RSA signature with structure, not a uniform key, and we need exactly
//! 32 bytes.

use keyring_crypto::{seal, Aad, Envelope, Key32, Purpose, ENVELOPE_VERSION, NO_SUBJECT};
use windows::core::HSTRING;
use windows::Security::Credentials::{
    KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
};
use windows::Storage::Streams::{DataReader, IBuffer};
use zeroize::Zeroizing;

use crate::platform::biometric::{BiometricError, BiometricKind, Biometrics};
use crate::platform::secure_store::SecureStore as _;
use crate::platform::windows::dpapi::DpapiStore;
use crate::platform::windows::winrt::{block_on, block_on_action};

/// Fixed challenge signed to derive the wrapping key.
///
/// Constant on purpose: the wrapping key must be reproducible across restarts.
/// Its secrecy is irrelevant — the security comes from the private key being
/// TPM-held and Hello-gated, not from the challenge being unpredictable.
const CHALLENGE: &[u8] = b"keyring/v1/windows-hello/wrap-challenge";

/// Domain string for turning a signature into a 32-byte key.
const HKDF_INFO: &[u8] = b"keyring/v1/biometric/wrap";

/// A reserved key id for biometric wraps, distinct from the vault's.
const BIOMETRIC_KEY_ID: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10];

/// Windows Hello biometrics.
///
/// Owns the DPAPI store the wrapped secret lands in, so the trait can present
/// the same "the platform holds it" shape macOS does.
pub struct WindowsHello {
    store: DpapiStore,
}

impl WindowsHello {
    /// A handle to the platform's Hello service.
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: DpapiStore::new(),
        }
    }

    /// A handle whose DPAPI blobs land under an explicit directory, for tests.
    #[must_use]
    pub fn with_store(store: DpapiStore) -> Self {
        Self { store }
    }

    /// Sign the fixed challenge with the credential named `label`, prompting.
    ///
    /// `create` decides whether a missing credential is created or is an error.
    fn wrapping_key(label: &str, create: bool) -> Result<Key32, BiometricError> {
        let name = HSTRING::from(label);

        let credential = if create {
            let op = KeyCredentialManager::RequestCreateAsync(
                &name,
                KeyCredentialCreationOption::ReplaceExisting,
            )
            .map_err(|_| BiometricError::Platform)?;
            let op = block_on(&op).map_err(|_| BiometricError::Platform)?;
            match op.Status() {
                Ok(KeyCredentialStatus::Success) => {
                    op.Credential().map_err(|_| BiometricError::Platform)?
                }
                Ok(KeyCredentialStatus::UserCanceled) => return Err(BiometricError::Cancelled),
                Ok(KeyCredentialStatus::NotFound) => return Err(BiometricError::Unavailable),
                _ => return Err(BiometricError::Platform),
            }
        } else {
            let op =
                KeyCredentialManager::OpenAsync(&name).map_err(|_| BiometricError::Platform)?;
            let op = block_on(&op).map_err(|_| BiometricError::Platform)?;
            match op.Status() {
                Ok(KeyCredentialStatus::Success) => {
                    op.Credential().map_err(|_| BiometricError::Platform)?
                }
                // The credential is gone. On Windows this is what an enrolment
                // change looks like, and it is the signal to fall back.
                Ok(KeyCredentialStatus::NotFound) => return Err(BiometricError::Invalidated),
                Ok(KeyCredentialStatus::UserCanceled) => return Err(BiometricError::Cancelled),
                _ => return Err(BiometricError::Platform),
            }
        };

        let challenge = buffer_from(CHALLENGE)?;
        let signed = credential
            .RequestSignAsync(&challenge)
            .map_err(|_| BiometricError::Platform)?;
        let signed = block_on(&signed).map_err(|_| BiometricError::Platform)?;

        match signed.Status() {
            Ok(KeyCredentialStatus::Success) => {}
            Ok(KeyCredentialStatus::UserCanceled) => return Err(BiometricError::Cancelled),
            Ok(KeyCredentialStatus::NotFound) => return Err(BiometricError::Invalidated),
            _ => return Err(BiometricError::Platform),
        }

        let signature = signed.Result().map_err(|_| BiometricError::Platform)?;
        let bytes = Zeroizing::new(buffer_to_vec(&signature)?);

        // The signature is structured RSA output, not a uniform key, so it is
        // extracted through HKDF rather than truncated.
        let mut ikm = Zeroizing::new([0u8; 32]);
        let digest = keyring_crypto::leaf_hash(&bytes);
        ikm.copy_from_slice(&digest);
        Ok(keyring_crypto::subkey::expand_for(&ikm, HKDF_INFO))
    }
}

impl Default for WindowsHello {
    fn default() -> Self {
        Self::new()
    }
}

fn buffer_from(bytes: &[u8]) -> Result<IBuffer, BiometricError> {
    use windows::Storage::Streams::DataWriter;
    let writer = DataWriter::new().map_err(|_| BiometricError::Platform)?;
    writer
        .WriteBytes(bytes)
        .map_err(|_| BiometricError::Platform)?;
    writer.DetachBuffer().map_err(|_| BiometricError::Platform)
}

fn buffer_to_vec(buffer: &IBuffer) -> Result<Vec<u8>, BiometricError> {
    let reader = DataReader::FromBuffer(buffer).map_err(|_| BiometricError::Platform)?;
    let len = buffer.Length().map_err(|_| BiometricError::Platform)?;
    let mut out = vec![0u8; len as usize];
    reader
        .ReadBytes(&mut out)
        .map_err(|_| BiometricError::Platform)?;
    Ok(out)
}

fn aad() -> Aad {
    Aad {
        envelope_version: ENVELOPE_VERSION,
        purpose: Purpose::AppCache,
        subject_id: NO_SUBJECT,
        revision: 0,
        key_id: BIOMETRIC_KEY_ID,
    }
}

impl Biometrics for WindowsHello {
    fn kind(&self) -> BiometricKind {
        BiometricKind::WindowsHello
    }

    fn is_available(&self) -> bool {
        KeyCredentialManager::IsSupportedAsync()
            .and_then(|op| block_on(&op))
            .unwrap_or(false)
    }

    fn enrol(&self, label: &str, secret: &[u8]) -> Result<(), BiometricError> {
        let key = Self::wrapping_key(label, true)?;
        let envelope = seal(&key, &aad(), secret).map_err(|_| BiometricError::Platform)?;
        // Two layers, and both earn their place: the AEAD binds the ciphertext
        // to a key only Hello releases, and DPAPI binds the file to this user on
        // this machine. Copying the file elsewhere defeats neither on its own.
        self.store
            .store(label, &envelope.to_bytes())
            .map_err(|_| BiometricError::Platform)
    }

    fn unwrap_secret(&self, label: &str) -> Result<Vec<u8>, BiometricError> {
        let wrapped = self
            .store
            .load(label)
            .map_err(|_| BiometricError::Invalidated)?
            .ok_or(BiometricError::Invalidated)?;
        let key = Self::wrapping_key(label, false)?;
        let envelope = Envelope::from_bytes(&wrapped).map_err(|_| BiometricError::Invalidated)?;
        let opened = keyring_crypto::open(&key, &aad(), &envelope)
            // A wrap that will not open under a freshly derived key means the
            // credential changed underneath us, which is the enrolment path.
            .map_err(|_| BiometricError::Invalidated)?;
        Ok(opened.to_vec())
    }

    fn revoke(&self, label: &str) -> Result<(), BiometricError> {
        let _ = self.store.delete(label);
        let op = KeyCredentialManager::DeleteAsync(&HSTRING::from(label))
            .map_err(|_| BiometricError::Platform)?;
        block_on_action(&op).map_err(|_| BiometricError::Platform)?;
        Ok(())
    }
}
