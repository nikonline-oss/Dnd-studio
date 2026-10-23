//! Secrets (ADR-0022): GM keys and access codes are stored as argon2 hashes (CLAUDE.md §6),
//! participant tokens as SHA-256 (they are 256 random bits; a slow hash adds nothing).

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use sha2::{Digest, Sha256};

/// Characters of access codes: no 0/O, 1/I/L, U — easy to read aloud and type.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
/// Access code length without the dash (~49 bits; joins are rate limited).
const CODE_LEN: usize = 10;

fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    // The OS random source does not fail on supported platforms; if it ever did, a zeroed
    // secret must not be handed out.
    #[allow(clippy::expect_used)]
    getrandom::fill(&mut bytes).expect("OS random source unavailable");
    bytes
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 256 random bits as hex: GM keys and participant tokens.
#[must_use]
pub fn new_token() -> String {
    hex(&random::<32>())
}

/// A human-friendly access code, `ABCDE-FGHJK`.
#[must_use]
pub fn new_access_code() -> String {
    let bytes = random::<CODE_LEN>();
    let chars: String = bytes
        .iter()
        // 256 % 30 = 16: the bias of the first 16 letters is negligible for a rate-limited code.
        .map(|b| char::from(CODE_ALPHABET[usize::from(*b) % CODE_ALPHABET.len()]))
        .collect();
    format!("{}-{}", &chars[..5], &chars[5..])
}

/// What users may type for a code: any case, with or without dashes and spaces.
#[must_use]
pub fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .flat_map(char::to_uppercase)
        .collect()
}

/// argon2id PHC string of `secret`. CPU-heavy: call from a blocking task.
pub fn hash_secret(secret: &str) -> anyhow::Result<String> {
    Ok(Argon2::default()
        .hash_password(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("argon2: {e}"))?
        .to_string())
}

/// Checks `secret` against an argon2 PHC string. CPU-heavy: call from a blocking task.
#[must_use]
pub fn verify_secret(secret: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(secret.as_bytes(), &hash)
            .is_ok()
    })
}

/// SHA-256 of a token, hex.
#[must_use]
pub fn token_hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// Compares two secrets without an early exit (on their hashes, so lengths don't leak either).
#[must_use]
pub fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (Sha256::digest(a.as_bytes()), Sha256::digest(b.as_bytes()));
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn codes_are_readable_and_forgiving() {
        let code = new_access_code();
        assert_eq!(code.len(), 11);
        assert_eq!(&code[5..6], "-");
        assert!(
            code.replace('-', "")
                .bytes()
                .all(|b| CODE_ALPHABET.contains(&b))
        );
        assert_eq!(normalize_code(" abcde-fghjk "), "ABCDEFGHJK");
        assert_ne!(new_access_code(), new_access_code());
    }

    #[test]
    fn hashes_verify_only_the_secret() {
        let phc = hash_secret("ABCDEFGHJK").unwrap();
        assert!(phc.starts_with("$argon2id$"));
        assert!(verify_secret("ABCDEFGHJK", &phc));
        assert!(!verify_secret("ABCDEFGHJX", &phc));
        assert!(!verify_secret("ABCDEFGHJK", "not a hash"));
        assert_eq!(new_token().len(), 64);
        assert_eq!(token_hash("t"), token_hash("t"));
        assert!(same_secret("host-token", "host-token"));
        assert!(!same_secret("host-token", "host-tokeN"));
    }
}
