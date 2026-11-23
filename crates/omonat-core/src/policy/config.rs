//! Versiyalangan policy (Sharia board imzosi bilan faollashadi) va I13 config validatsiyasi.

use crate::model::{Actor, Approval, PartyId, Violation};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Fond policy'si. Kafolatlangan daromad uchun maydon yo'q va bo'lishi mumkin emas (I13):
/// noma'lum maydonlar rad etiladi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: u32,
    /// Bitta foyda taqsimotida rezervga ajratish mumkin bo'lgan maksimal ulush (bps, foydadan).
    pub reserve_cap_bps: u32,
    /// Mudarib ulushi: rezervdan keyingi foydaning kelishilgan foizi (bps).
    pub mudarib_share_bps: u32,
    /// I8: bu oluvchilarga to'lov uchun operator + Sharia board imzosi kerak.
    #[serde(default)]
    pub related_parties: Vec<PartyId>,
    /// I7: Charity faqat shu oluvchilarga.
    #[serde(default)]
    pub approved_charities: Vec<PartyId>,
    /// Investorga ko'rsatiladigan mahsulot tavsifi.
    #[serde(default)]
    pub product_description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRecord {
    pub policy: Policy,
    pub proposed_by: Actor,
    pub proposed_at: DateTime<Utc>,
    pub activation: Option<Approval>,
}

const FORBIDDEN_KEYS: &[&str] = &[
    "guarant", "kafolat", "fixed_return", "fixed_rate", "min_return", "minimum_return",
    "return_rate", "expected_return", "promised",
];

const FORBIDDEN_PHRASES: &[&str] = &[
    "kafolatlangan",
    "kafolatli daromad",
    "kafolatli foyda",
    "qat'iy daromad",
    "qat'iy foyda",
    "guaranteed",
    "fixed return",
    "fixed income",
    "гарантирован",
    "фиксированн",
];

const NEGATED: &[&str] = &["not guaranteed", "non-guaranteed", "no guaranteed", "не гарантирован"];

fn i13(reason: impl Into<String>) -> Violation {
    Violation { rule_id: "I13".into(), reason: reason.into() }
}

/// Matnda kafolatlangan daromad va'dasi bormi.
pub fn promises_guaranteed_return(text: &str) -> Option<&'static str> {
    let mut t = text.to_lowercase();
    for n in NEGATED {
        t = t.replace(n, "");
    }
    FORBIDDEN_PHRASES.iter().copied().find(|p| t.contains(p))
}

impl Policy {
    pub fn validate(&self) -> Result<(), Vec<Violation>> {
        let mut v = Vec::new();
        if self.reserve_cap_bps > 10_000 {
            v.push(Violation { rule_id: "I5".into(), reason: "reserve_cap_bps 10000 dan oshmasligi kerak".into() });
        }
        if self.mudarib_share_bps > 10_000 {
            v.push(Violation { rule_id: "I6".into(), reason: "mudarib_share_bps 10000 dan oshmasligi kerak".into() });
        }
        if let Some(p) = promises_guaranteed_return(&self.product_description) {
            v.push(i13(format!("Mahsulot tavsifida kafolatlangan daromad va'dasi: \"{p}\"")));
        }
        if v.is_empty() { Ok(()) } else { Err(v) }
    }

    /// JSON'dan o'qish. Kafolatlangan daromadga oid kalit bo'lsa — I13, noma'lum kalit bo'lsa — rad.
    pub fn from_json(json: &str) -> Result<Policy, Vec<Violation>> {
        let raw: serde_json::Value =
            serde_json::from_str(json).map_err(|e| vec![Violation { rule_id: "CONFIG".into(), reason: e.to_string() }])?;
        let mut v = Vec::new();
        collect_forbidden_keys(&raw, &mut v);
        if !v.is_empty() {
            return Err(v);
        }
        let policy: Policy = serde_json::from_value(raw)
            .map_err(|e| vec![Violation { rule_id: "CONFIG".into(), reason: e.to_string() }])?;
        policy.validate()?;
        Ok(policy)
    }
}

fn collect_forbidden_keys(v: &serde_json::Value, out: &mut Vec<Violation>) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                let lk = k.to_lowercase();
                if FORBIDDEN_KEYS.iter().any(|f| lk.contains(f)) {
                    out.push(i13(format!("Config'da kafolatlangan daromad maydoni bo'lishi mumkin emas: \"{k}\"")));
                }
                collect_forbidden_keys(val, out);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|i| collect_forbidden_keys(i, out)),
        _ => {}
    }
}
