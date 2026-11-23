//! Hash chain (I12) va kunlik Merkle root.

use crate::event::Event;
use chrono::{DateTime, SubsecRound, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const GENESIS_HASH: [u8; 32] = [0u8; 32];

/// Ledger yozuvi. `payload` — hodisaning aynan xeshlangan JSON matni.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub kind: String,
    pub payload: String,
    pub prev_hash: String,
    pub hash: String,
}

impl LedgerEntry {
    pub fn event(&self) -> Result<Event, serde_json::Error> {
        serde_json::from_str(&self.payload)
    }
}

/// Vaqtni mikrosekundgacha qisqartiradi (PostgreSQL timestamptz aniqligi), aks holda
/// saqlab-o'qilgan yozuv xeshi mos kelmay qoladi.
pub fn normalize_time(at: DateTime<Utc>) -> DateTime<Utc> {
    at.trunc_subsecs(6)
}

pub fn entry_hash(prev_hash: &[u8; 32], seq: u64, at: DateTime<Utc>, kind: &str, payload: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"omonat.ledger.v1");
    h.update(prev_hash);
    h.update(seq.to_be_bytes());
    h.update(at.timestamp_micros().to_be_bytes());
    h.update((kind.len() as u64).to_be_bytes());
    h.update(kind.as_bytes());
    h.update((payload.len() as u64).to_be_bytes());
    h.update(payload.as_bytes());
    h.finalize().into()
}

pub fn decode_hash(s: &str) -> Option<[u8; 32]> {
    let v = hex::decode(s).ok()?;
    v.try_into().ok()
}

pub fn make_entry(prev_hash: &[u8; 32], seq: u64, at: DateTime<Utc>, event: &Event) -> LedgerEntry {
    let at = normalize_time(at);
    let payload = serde_json::to_string(event).expect("event serializes");
    let kind = event.kind();
    let hash = entry_hash(prev_hash, seq, at, kind, &payload);
    LedgerEntry {
        seq,
        at,
        kind: kind.to_string(),
        payload,
        prev_hash: hex::encode(prev_hash),
        hash: hex::encode(hash),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("hash chain buzilgan: seq {seq}: {reason}")]
pub struct ChainError {
    pub seq: u64,
    pub reason: String,
}

/// Zanjirni boshidan tekshiradi: seq ketma-ketligi, prev_hash bog'lanishi, har bir yozuv xeshi.
pub fn verify_chain(entries: &[LedgerEntry]) -> Result<(), ChainError> {
    let mut prev = GENESIS_HASH;
    for (i, e) in entries.iter().enumerate() {
        let expected_seq = i as u64 + 1;
        if e.seq != expected_seq {
            return Err(ChainError { seq: e.seq, reason: format!("kutilgan seq {expected_seq}") });
        }
        if decode_hash(&e.prev_hash) != Some(prev) {
            return Err(ChainError { seq: e.seq, reason: "prev_hash oldingi yozuv xeshiga mos emas".into() });
        }
        let h = entry_hash(&prev, e.seq, e.at, &e.kind, &e.payload);
        if decode_hash(&e.hash) != Some(h) {
            return Err(ChainError { seq: e.seq, reason: "yozuv xeshi mazmuniga mos emas (yozuv o'zgartirilgan)".into() });
        }
        prev = h;
    }
    Ok(())
}

/// Merkle root: barg = H(0x00 || entry_hash), tugun = H(0x01 || chap || o'ng), toq bo'lsa oxirgisi ko'tariladi.
pub fn merkle_root(hashes: &[[u8; 32]]) -> [u8; 32] {
    if hashes.is_empty() {
        return [0u8; 32];
    }
    let mut level: Vec<[u8; 32]> = hashes
        .iter()
        .map(|h| {
            let mut d = Sha256::new();
            d.update([0u8]);
            d.update(h);
            d.finalize().into()
        })
        .collect();
    while level.len() > 1 {
        level = level
            .chunks(2)
            .map(|pair| {
                if pair.len() == 1 {
                    pair[0]
                } else {
                    let mut d = Sha256::new();
                    d.update([1u8]);
                    d.update(pair[0]);
                    d.update(pair[1]);
                    d.finalize().into()
                }
            })
            .collect();
    }
    level[0]
}

/// `from..=to` oralig'idagi yozuvlarning Merkle root'i (hex). Xesh buzilgan bo'lsa ham
/// yozuv mazmunidan qayta hisoblanadi — aynan shu anchor bilan solishtiriladi.
pub fn merkle_root_of(entries: &[LedgerEntry], from_seq: u64, to_seq: u64) -> String {
    let mut prev = GENESIS_HASH;
    let mut leaves = Vec::new();
    for e in entries {
        let h = entry_hash(&prev, e.seq, e.at, &e.kind, &e.payload);
        if e.seq >= from_seq && e.seq <= to_seq {
            leaves.push(h);
        }
        prev = h;
    }
    hex::encode(merkle_root(&leaves))
}
