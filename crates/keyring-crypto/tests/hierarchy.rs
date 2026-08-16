//! The key hierarchy as a whole (SPEC-V1 §3.1).
//!
//! The individual primitives are covered by `kat.rs`. This file checks the thing
//! that no published vector can: that we wired them together correctly, and in
//! particular that no derived key is ever reused across purposes.

use std::collections::HashSet;

use keyring_crypto::{
    derive_activity_subkey, derive_item_subkey, derive_muk, derive_subkey, open, seal,
    verifier_from, verify_password, Aad, AccountKeys, CryptoError, ItemSubkey, KdfParams, Key32,
    Muk, Purpose, Subkey, ENVELOPE_VERSION,
};

const SALT: [u8; 32] = [0x5a; 32];
const PASSWORD: &[u8] = b"a generated fixture master password";

fn muk() -> Muk {
    derive_muk(PASSWORD, &SALT, KdfParams::floor()).expect("derive")
}

#[test]
fn every_muk_subkey_is_distinct() {
    let muk = muk();
    let all = [
        Subkey::Verify,
        Subkey::Header,
        Subkey::Wrap,
        Subkey::Vault,
        Subkey::AppCache,
    ];

    let mut seen: HashSet<[u8; 32]> = HashSet::new();
    for which in all {
        let key = derive_subkey(&muk, which);
        assert!(
            seen.insert(*key.expose()),
            "{which:?} collides with another subkey — a key reused across purposes"
        );
        assert_ne!(key.expose(), muk.expose(), "{which:?} is the MUK itself");
    }
    assert_eq!(seen.len(), all.len());
}

#[test]
fn info_strings_are_the_documented_literals() {
    // These strings are effectively part of the on-disk format: change one and
    // every existing vault derives different subkeys and stops opening.
    assert_eq!(Subkey::Verify.info(), b"keyring/v1/muk/verify");
    assert_eq!(Subkey::Header.info(), b"keyring/v1/muk/header");
    assert_eq!(Subkey::Wrap.info(), b"keyring/v1/muk/wrap");
    assert_eq!(Subkey::Vault.info(), b"keyring/v1/muk/vault");
    assert_eq!(Subkey::AppCache.info(), b"keyring/v1/muk/appcache");
    assert_eq!(ItemSubkey::Meta.info(), b"keyring/v1/item/meta");
    assert_eq!(ItemSubkey::Secret.info(), b"keyring/v1/item/secret");
    assert_eq!(
        keyring_crypto::subkey::INFO_VAULT_ACTIVITY,
        b"keyring/v1/vault/activity"
    );
}

#[test]
fn an_items_meta_and_secret_subkeys_are_distinct() {
    // The whole point of SPEC-V1 §3.4: unlocking decrypts meta for every item,
    // and must not thereby be able to decrypt any secret.
    let item_key = Key32::from_bytes([0x77; 32]);
    let meta = derive_item_subkey(&item_key, ItemSubkey::Meta);
    let secret = derive_item_subkey(&item_key, ItemSubkey::Secret);

    assert_ne!(meta.expose(), secret.expose());
    assert_ne!(meta.expose(), item_key.expose());
    assert_ne!(secret.expose(), item_key.expose());
}

#[test]
fn the_meta_key_cannot_open_the_secret_envelope() {
    let item_key = Key32::from_bytes([0x77; 32]);
    let meta_key = derive_item_subkey(&item_key, ItemSubkey::Meta);
    let secret_key = derive_item_subkey(&item_key, ItemSubkey::Secret);

    let secret_aad = Aad {
        envelope_version: ENVELOPE_VERSION,
        purpose: Purpose::ItemSecret,
        subject_id: [0x01; 16],
        revision: 1,
        key_id: [0x02; 16],
    };
    let sealed = seal(&secret_key, &secret_aad, b"the password").expect("seal");

    assert_eq!(
        open(&meta_key, &secret_aad, &sealed),
        Err(CryptoError::Authentication),
        "the metadata key opened a secret envelope"
    );
    assert!(open(&secret_key, &secret_aad, &sealed).is_ok());
}

#[test]
fn a_vaults_activity_subkey_is_distinct_from_its_vault_key() {
    let vault_key = Key32::from_bytes([0x88; 32]);
    let activity = derive_activity_subkey(&vault_key);
    assert_ne!(activity.expose(), vault_key.expose());

    let other_vault = Key32::from_bytes([0x89; 32]);
    assert_ne!(
        activity.expose(),
        derive_activity_subkey(&other_vault).expose(),
        "two vaults share an activity key"
    );
}

#[test]
fn subkeys_are_deterministic_across_derivations() {
    let a = derive_subkey(&muk(), Subkey::Vault);
    let b = derive_subkey(&muk(), Subkey::Vault);
    assert_eq!(a.expose(), b.expose());
}

#[test]
fn the_verifier_confirms_the_right_password_and_only_that() {
    let stored = verifier_from(&muk());
    assert!(verify_password(&muk(), &stored));

    let wrong = derive_muk(b"not the password", &SALT, KdfParams::floor()).expect("derive");
    assert!(!verify_password(&wrong, &stored));

    // Same password, different vault: the salt must make the verifier useless
    // across vaults.
    let other_vault = derive_muk(PASSWORD, &[0x5b; 32], KdfParams::floor()).expect("derive");
    assert!(!verify_password(&other_vault, &stored));
}

#[test]
fn the_verifier_is_not_the_muk_and_not_a_wrapping_key() {
    // Storing something derived from the MUK in the clear is only safe if it is
    // not itself a key that opens anything.
    let muk = muk();
    let stored = verifier_from(&muk);
    assert_ne!(&stored, muk.expose());
    assert_ne!(&stored, derive_subkey(&muk, Subkey::Wrap).expose());
    assert_ne!(&stored, derive_subkey(&muk, Subkey::Vault).expose());
    assert_ne!(&stored, derive_subkey(&muk, Subkey::Header).expose());
    assert_ne!(&stored, derive_subkey(&muk, Subkey::AppCache).expose());
}

#[test]
fn the_account_key_bundle_survives_a_seal_and_open_under_muk_wrap() {
    // V1 generates the bundle even though only the Ed25519 half is used, because
    // retrofitting identity keys later is a migration nobody wants to write.
    let muk = muk();
    let wrap = derive_subkey(&muk, Subkey::Wrap);
    let keys = AccountKeys::generate().expect("keys");
    let public = keys.public();

    let aad = Aad {
        envelope_version: ENVELOPE_VERSION,
        purpose: Purpose::AppCache,
        subject_id: keyring_crypto::NO_SUBJECT,
        revision: 0,
        key_id: keyring_crypto::reserved_key_id::MUK_WRAP,
    };

    let sealed = seal(&wrap, &aad, keys.to_bytes().as_ref()).expect("seal");
    let opened = open(&wrap, &aad, &sealed).expect("open");
    let restored = AccountKeys::from_bytes(&opened).expect("restore");

    assert_eq!(restored.public(), public);

    // And the restored key still signs verifiably under the stored public half.
    let sig = restored.sign(b"manifest root stand-in");
    keyring_crypto::keys::verify_ed25519(&public.ed25519, b"manifest root stand-in", &sig)
        .expect("verify");
}

#[test]
fn account_keys_reject_a_wrong_length_bundle() {
    assert_eq!(
        AccountKeys::from_bytes(&[0u8; 63]).unwrap_err(),
        CryptoError::InvalidLength
    );
    assert_eq!(
        AccountKeys::from_bytes(&[0u8; 65]).unwrap_err(),
        CryptoError::InvalidLength
    );
    assert!(AccountKeys::from_bytes(&[0u8; 64]).is_ok());
}

#[test]
fn reserved_key_ids_are_the_documented_values() {
    use keyring_crypto::reserved_key_id::{MUK_APPCACHE, MUK_HEADER, MUK_WRAP};
    assert_eq!(MUK_WRAP[15], 1);
    assert_eq!(MUK_APPCACHE[15], 2);
    assert_eq!(MUK_HEADER[15], 3);
    assert_eq!(&MUK_WRAP[..15], &[0u8; 15]);
    assert_eq!(&MUK_APPCACHE[..15], &[0u8; 15]);
    assert_eq!(&MUK_HEADER[..15], &[0u8; 15]);
    // All distinct, so an envelope's key_id names exactly one key.
    assert_ne!(MUK_WRAP, MUK_APPCACHE);
    assert_ne!(MUK_APPCACHE, MUK_HEADER);
    assert_ne!(MUK_WRAP, MUK_HEADER);
}
