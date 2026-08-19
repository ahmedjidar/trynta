//! Known-answer tests against published vectors.
//!
//! CLAUDE.md §8. Every primitive in the key hierarchy is checked against a value
//! published by someone who is not us, so a wrong build fails loudly instead of
//! producing self-consistent nonsense that only this codebase can read.
//!
//! Sources, cited so anyone can re-derive them:
//!
//! - Argon2id: RFC 9106 §5.3
//! - HKDF: RFC 5869 §A.1, §A.2, §A.3
//! - AEAD: RFC 8439 §2.8.2 for ChaCha20-Poly1305, and
//!   draft-irtf-cfrg-xchacha-03 §A.3.1 for XChaCha20-Poly1305
//! - Ed25519: RFC 8032 §7.1 TEST 1
//! - X25519: RFC 7748 §6.1
//! - HMAC: RFC 4231 §4.2 TC1
//! - `BLAKE2b`: the reference implementation's published digests

use argon2::{Algorithm, Argon2, AssociatedData, ParamsBuilder, Version};
use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use hex_literal::hex;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

// ── Argon2id — RFC 9106 §5.3 ─────────────────────────────────────────────────

#[test]
fn argon2id_rfc9106_section_5_3() {
    let params = ParamsBuilder::new()
        .m_cost(32)
        .t_cost(3)
        .p_cost(4)
        .output_len(32)
        .data(AssociatedData::new(&[0x04; 12]).expect("associated data"))
        .build()
        .expect("params");

    let argon = Argon2::new_with_secret(&[0x03; 8], Algorithm::Argon2id, Version::V0x13, params)
        .expect("argon2 with secret");

    let mut out = [0u8; 32];
    argon
        .hash_password_into(&[0x01; 32], &[0x02; 16], &mut out)
        .expect("hash");

    assert_eq!(
        out,
        hex!("0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659")
    );
}

#[test]
fn argon2id_is_not_argon2i_or_argon2d() {
    // The same inputs under the other two variants, so a mixed-up Algorithm
    // constant cannot pass the test above by coincidence.
    let build = |alg| {
        let params = ParamsBuilder::new()
            .m_cost(32)
            .t_cost(3)
            .p_cost(4)
            .output_len(32)
            .data(AssociatedData::new(&[0x04; 12]).expect("associated data"))
            .build()
            .expect("params");
        let argon =
            Argon2::new_with_secret(&[0x03; 8], alg, Version::V0x13, params).expect("argon2");
        let mut out = [0u8; 32];
        argon
            .hash_password_into(&[0x01; 32], &[0x02; 16], &mut out)
            .expect("hash");
        out
    };

    assert_eq!(
        build(Algorithm::Argon2d),
        hex!("512b391b6f1162975371d30919734294f868e3be3984f3c1a13a4db9fabe4acb")
    );
    assert_eq!(
        build(Algorithm::Argon2i),
        hex!("c814d9d1dc7f37aa13f0d77f2494bda1c8de6b016dd388d29952a4c4672b6ce8")
    );
}

// ── HKDF-SHA256 — RFC 5869 ───────────────────────────────────────────────────

#[test]
fn hkdf_sha256_rfc5869_case_1() {
    let ikm = hex!("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
    let salt = hex!("000102030405060708090a0b0c");
    let info = hex!("f0f1f2f3f4f5f6f7f8f9");

    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut okm = [0u8; 42];
    hk.expand(&info, &mut okm).expect("expand");

    assert_eq!(
        okm,
        hex!(
            "3cb25f25faacd57a90434f64d0362f2a
             2d2d0a90cf1a5a4c5db02d56ecc4c5bf
             34007208d5b887185865"
        )
    );
}

#[test]
fn hkdf_sha256_rfc5869_case_3_no_salt_no_info() {
    // The shape Trynta actually uses: no salt (the IKM is already a uniform
    // 32-byte key) and separation carried entirely by `info`.
    let ikm = hex!("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut okm = [0u8; 42];
    hk.expand(&[], &mut okm).expect("expand");

    assert_eq!(
        okm,
        hex!(
            "8da4e775a563c18f715f802a063c5a31
             b8a11f5c5ee1879ec3454e5f3c738d2d
             9d201395faa4b61a96c8"
        )
    );
}

// ── AEAD — RFC 8439 and draft-irtf-cfrg-xchacha ──────────────────────────────

#[test]
fn chacha20poly1305_rfc8439_section_2_8_2() {
    let key = hex!("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
    let nonce = hex!("070000004041424344454647");
    let aad = hex!("50515253c0c1c2c3c4c5c6c7");
    let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";

    let ct = ChaCha20Poly1305::new(&key.into())
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .expect("seal");

    assert_eq!(
        ct,
        hex!(
            "d31a8d34648e60db7b86afbc53ef7ec2
             a4aded51296e08fea9e2b5a736ee62d6
             3dbea45e8ca9671282fafb69da92728b
             1a71de0a9e060b2905d6a5b67ecd3b36
             92ddbd7f2d778b8c9803aee328091b58
             fab324e4fad675945585808b4831d7bc
             3ff4def08e4b7a9de576d26586cec64b
             6116
             1ae10b594f09e26a7e902ecbd0600691"
        )
        .to_vec()
    );
}

#[test]
fn xchacha20poly1305_draft_irtf_cfrg_xchacha_a_3_1() {
    let key = hex!("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
    let nonce = hex!("404142434445464748494a4b4c4d4e4f5051525354555657");
    let aad = hex!("50515253c0c1c2c3c4c5c6c7");
    let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";

    let ct = XChaCha20Poly1305::new(&key.into())
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .expect("seal");

    assert_eq!(
        ct,
        hex!(
            "bd6d179d3e83d43b9576579493c0e939
             572a1700252bfaccbed2902c21396cbb
             731c7f1b0b4aa6440bf3a82f4eda7e39
             ae64c6708c54c216cb96b72e1213b452
             2f8c9ba40db5d945b11b69b982c1bb9e
             3f3fac2bc369488f76b2383565d3fff9
             21f9664c97637da9768812f615c68b13
             b52e
             c0875924c1c7987947deafd8780acf49"
        )
        .to_vec()
    );
}

// ── Ed25519 — RFC 8032 §7.1 TEST 1 ───────────────────────────────────────────

#[test]
fn ed25519_rfc8032_test_1() {
    let secret = hex!("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
    let expected_public = hex!("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");

    let sk = SigningKey::from_bytes(&secret);
    assert_eq!(sk.verifying_key().to_bytes(), expected_public);

    let sig = sk.sign(b"");
    assert_eq!(
        sig.to_bytes(),
        hex!(
            "e5564300c360ac729086e2cc806e828a
             84877f1eb8e5d974d873e06522490155
             5fb8821590a33bacc61e39701cf9b46b
             d25bf5f0595bbe24655141438e7a100b"
        )
    );

    VerifyingKey::from_bytes(&expected_public)
        .expect("public key")
        .verify(b"", &sig)
        .expect("verify");
}

// ── X25519 — RFC 7748 §6.1 ───────────────────────────────────────────────────

#[test]
fn x25519_rfc7748_section_6_1() {
    let alice_sk = hex!("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
    let bob_sk = hex!("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");

    let a = x25519_dalek::StaticSecret::from(alice_sk);
    let b = x25519_dalek::StaticSecret::from(bob_sk);

    assert_eq!(
        x25519_dalek::PublicKey::from(&a).to_bytes(),
        hex!("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a")
    );
    assert_eq!(
        x25519_dalek::PublicKey::from(&b).to_bytes(),
        hex!("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f")
    );

    let shared = hex!("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
    assert_eq!(
        a.diffie_hellman(&x25519_dalek::PublicKey::from(&b))
            .to_bytes(),
        shared
    );
    assert_eq!(
        b.diffie_hellman(&x25519_dalek::PublicKey::from(&a))
            .to_bytes(),
        shared
    );
}

// ── HMAC-SHA256 — RFC 4231 §4.2 ──────────────────────────────────────────────

#[test]
fn hmac_sha256_rfc4231_case_1() {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&[0x0b; 20]).expect("key");
    mac.update(b"Hi There");
    assert_eq!(
        mac.finalize().into_bytes().as_slice(),
        hex!("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
    );
}

// ── BLAKE2b-256 ──────────────────────────────────────────────────────────────

#[test]
fn blake2b_256_reference_digests() {
    let mut h = Blake2b::<U32>::new();
    h.update(b"abc");
    assert_eq!(
        h.finalize().as_slice(),
        hex!("bddd813c634239723171ef3fee98579b94964e3bb1cb3e427262c8c068d52319")
    );

    let mut h = Blake2b::<U32>::new();
    h.update(b"");
    assert_eq!(
        h.finalize().as_slice(),
        hex!("0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8")
    );
}
