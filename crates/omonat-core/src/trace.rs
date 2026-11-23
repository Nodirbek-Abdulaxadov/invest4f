//! Lineage (backward) va forward tracing.

use crate::model::*;
use crate::money::Money;
use crate::state::State;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageStep {
    pub lot_id: LotId,
    pub class: MoneyClass,
    pub owner: Option<PartyId>,
    pub amount: Money,
    pub contract_id: Option<ContractId>,
    pub tx_id: TxId,
    pub tx_kind: Option<TxKind>,
    pub bank_ref: Option<String>,
    pub at: DateTime<Utc>,
}

/// Lotdan ildizgacha (bankdan kirgan pulgacha) zanjir. Birinchi element — lotning o'zi.
pub fn lineage(s: &State, lot_id: LotId) -> Vec<LineageStep> {
    let mut out = Vec::new();
    let mut cur = s.lot(lot_id);
    while let Some(l) = cur {
        let tx = s.txs.get(&l.tx_id);
        out.push(LineageStep {
            lot_id: l.id,
            class: l.class,
            owner: l.owner,
            amount: l.amount,
            contract_id: l.contract_id,
            tx_id: l.tx_id,
            tx_kind: tx.map(|t| t.proposal.kind),
            bank_ref: tx.and_then(|t| t.proposal.bank_ref.clone()),
            at: l.created_at,
        });
        cur = l.parent_id.and_then(|p| s.lot(p));
    }
    out
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaymentBranch {
    pub amount: Money,
    pub lineage: Vec<LineageStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaymentLineage {
    pub payment: Payment,
    pub branches: Vec<PaymentBranch>,
}

pub fn payment_lineage(s: &State, payment_id: PaymentId) -> Option<PaymentLineage> {
    let payment = s.payments.get(&payment_id)?.clone();
    let branches = payment
        .sources
        .iter()
        .map(|(lot, amount)| PaymentBranch { amount: *amount, lineage: lineage(s, *lot) })
        .collect();
    Some(PaymentLineage { payment, branches })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TraceNode {
    Lot { lot_id: LotId, hop: u32 },
    Payment { payment_id: PaymentId, hop: u32 },
}

/// Forward trace: lotdan bolalari, undan to'langan to'lovlar va (Deploy bo'lsa) shu egaga
/// shartnomadan qaytgan lotlar. `hop_limit` va vaqt oynasi bilan cheklangan.
pub fn forward(s: &State, root: LotId, hop_limit: u32, window_days: u32) -> Vec<TraceNode> {
    let Some(root_lot) = s.lot(root) else { return Vec::new() };
    let deadline = root_lot.created_at + Duration::days(window_days as i64);
    let mut out = Vec::new();
    let mut seen_lots = BTreeSet::new();
    let mut seen_pay = BTreeSet::new();
    let mut q: VecDeque<(LotId, u32)> = VecDeque::new();
    q.push_back((root, 0));
    seen_lots.insert(root);
    while let Some((id, hop)) = q.pop_front() {
        out.push(TraceNode::Lot { lot_id: id, hop });
        if hop >= hop_limit {
            continue;
        }
        let owner = s.lot(id).and_then(|l| l.owner);
        for child in s.lot_children.get(&id).into_iter().flatten() {
            if s.lot(*child).map(|l| l.created_at <= deadline).unwrap_or(false) && seen_lots.insert(*child) {
                q.push_back((*child, hop + 1));
            }
        }
        for pid in s.lot_payments.get(&id).into_iter().flatten() {
            let Some(pay) = s.payments.get(pid) else { continue };
            if pay.created_at > deadline || !seen_pay.insert(*pid) {
                continue;
            }
            out.push(TraceNode::Payment { payment_id: *pid, hop: hop + 1 });
            if pay.kind != TxKind::Deploy || hop + 2 > hop_limit {
                continue;
            }
            let Some(c) = s.contracts.values().find(|c| c.purchase_tx == Some(pay.tx_id)) else { continue };
            for l in s.lots.values() {
                let from_contract = l.contract_id == Some(c.id) && l.owner == owner && owner.is_some() && l.parent_id.is_none()
                    || l.contract_id == Some(c.id)
                        && l.owner == owner
                        && owner.is_some()
                        && l.class == MoneyClass::InvestorProfit;
                if from_contract && l.created_at <= deadline && seen_lots.insert(l.id) {
                    q.push_back((l.id, hop + 2));
                }
            }
        }
    }
    out
}
