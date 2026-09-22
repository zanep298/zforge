//! Content hash of an intake file (D1): SHA-256 of the text with line
//! endings normalized to LF, so a CRLF editor save is not a new revision.

use sha2::{Digest, Sha256};

pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

pub fn sha256(text: &str) -> String {
    let digest = Sha256::digest(normalize(text).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// First 12 hex characters, for display.
pub fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_sha256_and_ignores_line_endings() {
        assert_eq!(
            sha256("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256("a\r\nb\r\n"), sha256("a\nb\n"));
        assert_ne!(sha256("a\nb\n"), sha256("a\nb"));
    }
}
