//! Ed25519 signatures for packages and the catalog index (ADR-0060).
//!
//! Both signatures are detached: a package is signed over a short transcript, the index over its
//! exact bytes. The format is fixed by ADR-0060 so the publishing side and the client agree on
//! what a signature proves.

use ring::signature::{ED25519, UnparsedPublicKey};

use crate::error::{PluginError, PluginResult};

/// Domain prefix of a package signature transcript.
pub const PLUGIN_DOMAIN: &str = "dndstudio-plugin-v1";
/// Domain prefix of the catalog index signature.
pub const INDEX_DOMAIN: &str = "dndstudio-index-v1";

/// The exact bytes a package signature covers: domain, id, version and archive hash.
///
/// Binding identity and version to the hash means a signature cannot be moved to another plugin
/// or another release, even though the archive is already content-addressed.
#[must_use]
pub fn plugin_transcript(id: &str, version: &str, sha256_hex: &str) -> Vec<u8> {
    format!("{PLUGIN_DOMAIN}\n{id}\n{version}\n{sha256_hex}").into_bytes()
}

/// The exact bytes the index signature covers.
#[must_use]
pub fn index_transcript(index_bytes: &[u8]) -> Vec<u8> {
    let prefix = format!("{INDEX_DOMAIN}\n");
    let mut bytes = Vec::with_capacity(prefix.len() + index_bytes.len());
    bytes.extend_from_slice(prefix.as_bytes());
    bytes.extend_from_slice(index_bytes);
    bytes
}

/// Checks a hex-encoded Ed25519 signature over `message` with a 32-byte public key.
pub fn verify(public_key: &[u8], message: &[u8], signature_hex: &str) -> PluginResult<()> {
    if public_key.len() != 32 {
        return Err(PluginError::SignatureInvalid {
            reason: format!("a public key is 32 bytes, got {}", public_key.len()),
        });
    }
    let signature = decode_hex(signature_hex)?;
    if signature.len() != 64 {
        return Err(PluginError::SignatureInvalid {
            reason: format!("a signature is 64 bytes, got {}", signature.len()),
        });
    }
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(message, &signature)
        .map_err(|_| PluginError::SignatureInvalid {
            reason: "the signature does not match the signed bytes".to_owned(),
        })
}

/// Verifies a package signature: `id` and `version` must be the ones in the manifest.
pub fn verify_plugin(
    public_key: &[u8],
    id: &str,
    version: &str,
    sha256_hex: &str,
    signature_hex: &str,
) -> PluginResult<()> {
    verify(
        public_key,
        &plugin_transcript(id, version, sha256_hex),
        signature_hex,
    )
}

/// Verifies the index signature over its exact file bytes.
pub fn verify_index(
    public_key: &[u8],
    index_bytes: &[u8],
    signature_hex: &str,
) -> PluginResult<()> {
    verify(public_key, &index_transcript(index_bytes), signature_hex)
}

/// Hexadecimal, lower case, the format both signatures use.
#[must_use]
pub fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn decode_hex(text: &str) -> PluginResult<Vec<u8>> {
    let invalid = || PluginError::SignatureInvalid {
        reason: "the signature is not lower-case hexadecimal".to_owned(),
    };
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return Err(invalid());
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).map_err(|_| invalid()))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair as _};

    fn key_pair() -> (ring::signature::Ed25519KeyPair, Vec<u8>) {
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
        let public = pair.public_key().as_ref().to_vec();
        (pair, public)
    }

    #[test]
    fn a_package_signature_binds_id_version_and_hash() {
        let (pair, public) = key_pair();
        let hash = "a".repeat(64);
        let signature = encode_hex(
            pair.sign(&plugin_transcript("srd5", "1.0.0", &hash))
                .as_ref(),
        );
        verify_plugin(&public, "srd5", "1.0.0", &hash, &signature).unwrap();
        // A different version, id or archive is not covered by this signature.
        assert!(verify_plugin(&public, "srd5", "1.0.1", &hash, &signature).is_err());
        assert!(verify_plugin(&public, "other", "1.0.0", &hash, &signature).is_err());
        assert!(verify_plugin(&public, "srd5", "1.0.0", &"b".repeat(64), &signature).is_err());
        // Another key does not verify either.
        let (_, other_public) = key_pair();
        assert!(verify_plugin(&other_public, "srd5", "1.0.0", &hash, &signature).is_err());
    }

    #[test]
    fn an_index_signature_covers_the_exact_bytes() {
        let (pair, public) = key_pair();
        let bytes = br#"{"format":1,"plugins":[]}"#;
        let signature = encode_hex(pair.sign(&index_transcript(bytes)).as_ref());
        verify_index(&public, bytes, &signature).unwrap();
        assert!(verify_index(&public, br#"{"format":1,"plugins":[ ]}"#, &signature).is_err());
    }

    #[test]
    fn malformed_signatures_are_reported_not_accepted() {
        let (_, public) = key_pair();
        assert!(verify_plugin(&public, "id", "1.0.0", &"a".repeat(64), "zz").is_err());
        assert!(verify_plugin(&public, "id", "1.0.0", &"a".repeat(64), &"a".repeat(63)).is_err());
        assert!(
            verify_plugin(
                &public[..31],
                "id",
                "1.0.0",
                &"a".repeat(64),
                &"a".repeat(128)
            )
            .is_err()
        );
    }

    #[test]
    fn hex_round_trips() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(decode_hex(&encode_hex(&bytes)).unwrap(), bytes);
    }
}
