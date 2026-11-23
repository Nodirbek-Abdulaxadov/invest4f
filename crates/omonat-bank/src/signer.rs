//! Imzo kaliti. Production'da `Signer` HSM/KMS ustida ishlaydi; bu yerda dasturiy Ed25519
//! (faqat dev/test uchun — kalit seed'dan hosil qilinadi).

use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub trait Signer: Send + Sync {
    fn key_id(&self) -> &str;
    fn public_key_hex(&self) -> String;
    /// Imzo (hex).
    fn sign(&self, msg: &[u8]) -> String;
}

pub struct SoftwareSigner {
    key_id: String,
    key: SigningKey,
}

impl SoftwareSigner {
    /// Seed'dan deterministik kalit (dev). Seed sir saqlanishi kerak.
    pub fn from_seed(key_id: impl Into<String>, seed: &[u8]) -> Self {
        let digest: [u8; 32] = Sha256::digest(seed).into();
        SoftwareSigner { key_id: key_id.into(), key: SigningKey::from_bytes(&digest) }
    }
}

impl Signer for SoftwareSigner {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    fn public_key_hex(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }

    fn sign(&self, msg: &[u8]) -> String {
        hex::encode(self.key.sign(msg).to_bytes())
    }
}

pub fn verify(public_key_hex: &str, msg: &[u8], signature_hex: &str) -> bool {
    let Ok(pk) = hex::decode(public_key_hex) else { return false };
    let Ok(pk) = <[u8; 32]>::try_from(pk.as_slice()) else { return false };
    let Ok(vk) = VerifyingKey::from_bytes(&pk) else { return false };
    let Ok(sig) = hex::decode(signature_hex) else { return false };
    let Ok(sig) = <[u8; 64]>::try_from(sig.as_slice()) else { return false };
    vk.verify(msg, &Signature::from_bytes(&sig)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let s = SoftwareSigner::from_seed("k", b"seed");
        let sig = s.sign(b"hello");
        assert!(verify(&s.public_key_hex(), b"hello", &sig));
        assert!(!verify(&s.public_key_hex(), b"hellO", &sig));
    }
}
