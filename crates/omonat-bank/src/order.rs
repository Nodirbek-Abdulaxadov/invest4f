//! To'lov topshirig'i (payment order) va uning imzolari.

use crate::signer::Signer;
use chrono::{DateTime, Utc};
use omonat_core::model::{Party, Payment};
use omonat_core::Money;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentOrder {
    pub payment_id: Uuid,
    pub tx_id: Uuid,
    pub amount: Money,
    pub beneficiary_account: String,
    pub beneficiary_name: String,
    pub purpose: String,
    pub created_at: DateTime<Utc>,
}

impl PaymentOrder {
    pub fn from_payment(p: &Payment, beneficiary: &Party) -> Self {
        PaymentOrder {
            payment_id: p.id,
            tx_id: p.tx_id,
            amount: p.amount,
            beneficiary_account: beneficiary.bank_account.clone(),
            beneficiary_name: beneficiary.name.clone(),
            purpose: p.purpose.clone(),
            created_at: p.created_at,
        }
    }

    /// Imzolanadigan baytlar: maydonlar tartibi qat'iy bo'lgan JSON.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut v = b"omonat.payment-order.v1|".to_vec();
        v.extend(serde_json::to_vec(self).expect("order serializes"));
        v
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedOrder {
    pub order: PaymentOrder,
    pub operator_signature: Option<String>,
    pub omonat_signature: Option<String>,
}

/// Bank Gateway: operator va Omonat (HSM) imzosi bilan topshiriq.
pub fn sign_order(order: PaymentOrder, operator: &dyn Signer, omonat: &dyn Signer) -> SignedOrder {
    let bytes = order.signing_bytes();
    SignedOrder {
        operator_signature: Some(operator.sign(&bytes)),
        omonat_signature: Some(omonat.sign(&bytes)),
        order,
    }
}
