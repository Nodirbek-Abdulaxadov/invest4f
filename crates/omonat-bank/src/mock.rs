//! Hamkor bank simulyatori: nominal hisob, 2-of-2 mandat (operator + Omonat imzosi),
//! kiruvchi o'tkazmalar va hisob ko'chirmasi. Holat ixtiyoriy JSON faylda saqlanadi.

use crate::order::SignedOrder;
use crate::signer::verify;
use chrono::{DateTime, Utc};
use omonat_core::Money;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum BankError {
    #[error("mandat buzilgan: {0}")]
    MandateViolation(String),
    #[error("hisobda mablag' yetarli emas")]
    InsufficientFunds,
    #[error("bank rad etdi: {0}")]
    Declined(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mandate {
    pub operator_public_key: String,
    pub omonat_public_key: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    In,
    Out,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementLine {
    pub bank_ref: String,
    pub direction: Direction,
    pub amount: Money,
    pub counterparty: String,
    pub memo: String,
    pub payment_id: Option<Uuid>,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct BankState {
    balance: Money,
    seq: u64,
    executed: BTreeMap<Uuid, String>,
    statement: Vec<StatementLine>,
    rejected_orders: Vec<(Uuid, String)>,
    decline_next: Option<String>,
}

pub struct MockBank {
    mandate: Mandate,
    path: Option<PathBuf>,
    state: Mutex<BankState>,
}

impl MockBank {
    pub fn new(mandate: Mandate) -> Self {
        MockBank { mandate, path: None, state: Mutex::new(BankState::default()) }
    }

    /// Holatni faylda saqlaydigan bank (server qayta ishga tushganda ham qoldiq saqlanadi).
    pub fn persistent(mandate: Mandate, path: PathBuf) -> std::io::Result<Self> {
        let state = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).map_err(std::io::Error::other)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BankState::default(),
            Err(e) => return Err(e),
        };
        Ok(MockBank { mandate, path: Some(path), state: Mutex::new(state) })
    }

    fn save(&self, st: &BankState) {
        if let Some(p) = &self.path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let tmp = p.with_extension("tmp");
            if std::fs::write(&tmp, serde_json::to_vec_pretty(st).expect("serializes")).is_ok() {
                let _ = std::fs::rename(&tmp, p);
            }
        }
    }

    fn next_ref(st: &mut BankState, prefix: &str) -> String {
        st.seq += 1;
        format!("{prefix}-{:08}", st.seq)
    }

    /// To'lovni bajaradi. Ikkala imzo ham mandatdagi kalitlar bilan tekshiriladi; bitta
    /// payment_id ikki marta bajarilmaydi (idempotent).
    pub fn execute(&self, signed: &SignedOrder, now: DateTime<Utc>) -> Result<String, BankError> {
        let mut st = self.state.lock().expect("bank lock");
        if let Some(r) = st.executed.get(&signed.order.payment_id) {
            return Ok(r.clone());
        }
        let bytes = signed.order.signing_bytes();
        let check = |sig: &Option<String>, pk: &str, who: &str| -> Result<(), BankError> {
            match sig {
                None => Err(BankError::MandateViolation(format!("{who} imzosi yo'q"))),
                Some(s) if !verify(pk, &bytes, s) => Err(BankError::MandateViolation(format!("{who} imzosi yaroqsiz"))),
                Some(_) => Ok(()),
            }
        };
        let verdict = check(&signed.operator_signature, &self.mandate.operator_public_key, "operator")
            .and_then(|_| check(&signed.omonat_signature, &self.mandate.omonat_public_key, "Omonat"))
            .and_then(|_| match st.decline_next.take() {
                Some(reason) => Err(BankError::Declined(reason)),
                None => Ok(()),
            })
            .and_then(|_| if signed.order.amount > st.balance { Err(BankError::InsufficientFunds) } else { Ok(()) });
        if let Err(e) = verdict {
            st.rejected_orders.push((signed.order.payment_id, e.to_string()));
            self.save(&st);
            return Err(e);
        }
        let bank_ref = Self::next_ref(&mut st, "OUT");
        st.balance -= signed.order.amount;
        st.executed.insert(signed.order.payment_id, bank_ref.clone());
        st.statement.push(StatementLine {
            bank_ref: bank_ref.clone(),
            direction: Direction::Out,
            amount: signed.order.amount,
            counterparty: format!("{} ({})", signed.order.beneficiary_name, signed.order.beneficiary_account),
            memo: signed.order.purpose.clone(),
            payment_id: Some(signed.order.payment_id),
            at: now,
        });
        self.save(&st);
        Ok(bank_ref)
    }

    /// Nominal hisobga kirim (investor depoziti yoki mijoz to'lovi).
    pub fn incoming(&self, counterparty: &str, amount: Money, memo: &str, now: DateTime<Utc>) -> StatementLine {
        let mut st = self.state.lock().expect("bank lock");
        let bank_ref = Self::next_ref(&mut st, "IN");
        st.balance += amount;
        let line = StatementLine {
            bank_ref,
            direction: Direction::In,
            amount,
            counterparty: counterparty.to_string(),
            memo: memo.to_string(),
            payment_id: None,
            at: now,
        };
        st.statement.push(line.clone());
        self.save(&st);
        line
    }

    pub fn balance(&self) -> Money {
        self.state.lock().expect("bank lock").balance
    }

    pub fn statement(&self) -> Vec<StatementLine> {
        self.state.lock().expect("bank lock").statement.clone()
    }

    pub fn rejected_orders(&self) -> Vec<(Uuid, String)> {
        self.state.lock().expect("bank lock").rejected_orders.clone()
    }

    pub fn is_executed(&self, payment_id: Uuid) -> Option<String> {
        self.state.lock().expect("bank lock").executed.get(&payment_id).cloned()
    }

    /// Dev: ledger'da qayd etilmagan harakat (I11 namoyishi uchun).
    pub fn adjust(&self, delta: Money, memo: &str, now: DateTime<Utc>) {
        let mut st = self.state.lock().expect("bank lock");
        let bank_ref = Self::next_ref(&mut st, "ADJ");
        st.balance += delta;
        st.statement.push(StatementLine {
            bank_ref,
            direction: if delta.0 >= 0 { Direction::In } else { Direction::Out },
            amount: Money(delta.0.abs()),
            counterparty: "—".into(),
            memo: memo.to_string(),
            payment_id: None,
            at: now,
        });
        self.save(&st);
    }

    /// Dev: navbatdagi to'lovni rad etish (Compensation namoyishi uchun).
    pub fn decline_next(&self, reason: &str) {
        let mut st = self.state.lock().expect("bank lock");
        st.decline_next = Some(reason.to_string());
        self.save(&st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::{sign_order, PaymentOrder};
    use crate::signer::{Signer, SoftwareSigner};

    fn setup() -> (MockBank, SoftwareSigner, SoftwareSigner, PaymentOrder) {
        let operator = SoftwareSigner::from_seed("operator", b"operator-seed");
        let omonat = SoftwareSigner::from_seed("omonat-hsm", b"omonat-seed");
        let bank = MockBank::new(Mandate {
            operator_public_key: operator.public_key_hex(),
            omonat_public_key: omonat.public_key_hex(),
        });
        bank.incoming("Investor", Money::som(1_000_000), "depozit", Utc::now());
        let order = PaymentOrder {
            payment_id: Uuid::new_v4(),
            tx_id: Uuid::new_v4(),
            amount: Money::som(100_000),
            beneficiary_account: "20208000000000000001".into(),
            beneficiary_name: "Reklama agentligi".into(),
            purpose: "Xizmat haqi".into(),
            created_at: Utc::now(),
        };
        (bank, operator, omonat, order)
    }

    /// T12: operator Omonat imzosisiz bankka to'lov yubora olmaydi.
    #[test]
    fn t12_operator_cannot_pay_without_omonat_signature() {
        let (bank, operator, omonat, order) = setup();

        let only_operator = SignedOrder {
            operator_signature: Some(operator.sign(&order.signing_bytes())),
            omonat_signature: None,
            order: order.clone(),
        };
        assert!(matches!(bank.execute(&only_operator, Utc::now()), Err(BankError::MandateViolation(_))));

        // Operator o'z kalitini "Omonat imzosi" qilib qo'ysa ham o'tmaydi.
        let forged = SignedOrder { omonat_signature: only_operator.operator_signature.clone(), ..only_operator.clone() };
        assert!(matches!(bank.execute(&forged, Utc::now()), Err(BankError::MandateViolation(_))));

        // Imzolangandan keyin summani o'zgartirish ham o'tmaydi.
        let mut altered = sign_order(order.clone(), &operator, &omonat);
        altered.order.amount = Money::som(900_000);
        assert!(matches!(bank.execute(&altered, Utc::now()), Err(BankError::MandateViolation(_))));
        assert_eq!(bank.balance(), Money::som(1_000_000));

        let ok = sign_order(order, &operator, &omonat);
        let r1 = bank.execute(&ok, Utc::now()).unwrap();
        let r2 = bank.execute(&ok, Utc::now()).unwrap();
        assert_eq!(r1, r2, "idempotent");
        assert_eq!(bank.balance(), Money::som(900_000));
    }
}
