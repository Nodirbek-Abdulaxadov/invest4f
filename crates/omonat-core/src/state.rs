//! Ledger holati (ILedgerSnapshot): hodisalarni ketma-ket qo'llash natijasi.

use crate::event::Event;
use crate::model::*;
use crate::money::Money;
use crate::policy::config::{Policy, PolicyRecord};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingApproval {
    pub proposal: TxProposal,
    pub needs: Vec<NeedsApproval>,
    pub approvals: Vec<Approval>,
    pub policy_version: Option<u32>,
    pub created_at: DateTime<Utc>,
}

impl PendingApproval {
    pub fn missing_roles(&self) -> Vec<Role> {
        let mut out: Vec<Role> = Vec::new();
        for n in &self.needs {
            if !self.approvals.iter().any(|a| a.role == n.role) && !out.contains(&n.role) {
                out.push(n.role);
            }
        }
        out
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalOutcome {
    Executed,
    Denied,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedApproval {
    pub pending: PendingApproval,
    pub outcome: ApprovalOutcome,
    pub note: String,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub parties: BTreeMap<PartyId, Party>,
    pub policies: BTreeMap<u32, PolicyRecord>,
    pub active_policy: Option<u32>,

    pub lots: BTreeMap<LotId, Lot>,
    pub lot_status: BTreeMap<LotId, LotStatus>,
    pub lot_spent_by: BTreeMap<LotId, TxId>,
    pub lot_children: BTreeMap<LotId, Vec<LotId>>,
    pub lot_payments: BTreeMap<LotId, Vec<PaymentId>>,
    pub next_lot_seq: u64,

    pub txs: BTreeMap<TxId, AppliedTx>,
    pub tx_order: Vec<TxId>,
    pub payments: BTreeMap<PaymentId, Payment>,
    pub payment_status: BTreeMap<PaymentId, (PaymentStatus, Option<String>)>,

    pub contracts: BTreeMap<ContractId, Contract>,
    pub positions: BTreeMap<(ContractId, PartyId), Position>,

    pub pending: BTreeMap<TxId, PendingApproval>,
    pub approval_history: Vec<ResolvedApproval>,
    pub rejected: Vec<RejectedAttempt>,
    pub freezes: BTreeMap<CaseId, FreezeCase>,
    pub incidents: Vec<Incident>,
    pub reconciliations: Vec<Reconciliation>,
    pub anchors: Vec<Anchor>,

    pub head_seq: u64,
    pub head_hash: [u8; 32],
    pub last_at: Option<DateTime<Utc>>,
}

impl State {
    // ---------- o'qish ----------

    pub fn policy(&self) -> Option<&Policy> {
        self.active_policy.and_then(|v| self.policies.get(&v)).map(|r| &r.policy)
    }

    pub fn party(&self, id: PartyId) -> Option<&Party> {
        self.parties.get(&id)
    }

    pub fn is_party(&self, id: PartyId, kind: PartyKind) -> bool {
        self.parties.get(&id).map(|p| p.kind == kind).unwrap_or(false)
    }

    pub fn lot(&self, id: LotId) -> Option<&Lot> {
        self.lots.get(&id)
    }

    pub fn lot_status(&self, id: LotId) -> Option<LotStatus> {
        self.lot_status.get(&id).copied()
    }

    pub fn is_active(&self, id: LotId) -> bool {
        self.lot_status(id) == Some(LotStatus::Active)
    }

    /// Faol lotlar, FIFO (seq) tartibida.
    pub fn active_lots<'a>(&'a self, pred: impl Fn(&Lot) -> bool + 'a) -> Vec<&'a Lot> {
        let mut v: Vec<&Lot> = self.lots.values().filter(|l| self.is_active(l.id) && pred(l)).collect();
        v.sort_by_key(|l| l.seq);
        v
    }

    pub fn owner_lots(&self, owner: PartyId, class: MoneyClass) -> Vec<&Lot> {
        self.active_lots(move |l| l.owner == Some(owner) && l.class == class)
    }

    pub fn class_lots(&self, class: MoneyClass) -> Vec<&Lot> {
        self.active_lots(move |l| l.class == class)
    }

    pub fn sum(lots: &[&Lot]) -> Money {
        lots.iter().map(|l| l.amount).sum()
    }

    pub fn class_balance(&self, class: MoneyClass) -> Money {
        State::sum(&self.class_lots(class))
    }

    pub fn owner_balance(&self, owner: PartyId, class: MoneyClass) -> Money {
        State::sum(&self.owner_lots(owner, class))
    }

    pub fn frozen_balance(&self, owner: Option<PartyId>) -> Money {
        self.lots
            .values()
            .filter(|l| self.lot_status(l.id) == Some(LotStatus::Frozen) && (owner.is_none() || l.owner == owner))
            .map(|l| l.amount)
            .sum()
    }

    pub fn payment_status(&self, id: PaymentId) -> PaymentStatus {
        self.payment_status.get(&id).map(|s| s.0).unwrap_or(PaymentStatus::Pending)
    }

    /// Ledger bo'yicha nominal hisobda turishi kerak bo'lgan pul: sarflanmagan lotlar +
    /// bank hali bajarmagan chiquvchi to'lovlar (I11).
    pub fn expected_bank_balance(&self) -> Money {
        let lots: Money = self
            .lots
            .values()
            .filter(|l| matches!(self.lot_status(l.id), Some(LotStatus::Active | LotStatus::Frozen)))
            .map(|l| l.amount)
            .sum();
        let pending: Money = self
            .payments
            .values()
            .filter(|p| self.payment_status(p.id) == PaymentStatus::Pending)
            .map(|p| p.amount)
            .sum();
        lots + pending
    }

    pub fn contract(&self, id: ContractId) -> Option<&Contract> {
        self.contracts.get(&id)
    }

    pub fn contract_positions(&self, id: ContractId) -> Vec<&Position> {
        self.positions.range((id, PartyId::nil())..=(id, PartyId::max())).map(|(_, p)| p).collect()
    }

    pub fn investor_positions(&self, investor: PartyId) -> Vec<&Position> {
        self.positions.values().filter(|p| p.investor == investor).collect()
    }

    pub fn position(&self, contract: ContractId, investor: PartyId) -> Option<&Position> {
        self.positions.get(&(contract, investor))
    }

    // ---------- yozish ----------

    pub fn apply(&mut self, at: DateTime<Utc>, event: &Event) {
        self.last_at = Some(at);
        match event {
            Event::PartyRegistered { party, .. } => {
                self.parties.insert(party.id, party.clone());
            }
            Event::PolicyProposed { policy, by } => {
                self.policies.insert(
                    policy.version,
                    PolicyRecord { policy: policy.clone(), proposed_by: *by, proposed_at: at, activation: None },
                );
            }
            Event::PolicyActivated { version, approval } => {
                if let Some(r) = self.policies.get_mut(version) {
                    r.activation = Some(approval.clone());
                    self.active_policy = Some(*version);
                }
            }
            Event::ContractCreated { contract, .. } => {
                self.contracts.insert(contract.id, contract.clone());
            }
            Event::OwnershipDocAttached { contract_id, doc, .. } => {
                if let Some(c) = self.contracts.get_mut(contract_id) {
                    c.ownership_doc = Some(doc.clone());
                }
            }
            Event::ContractActivated { contract_id, .. } => {
                if let Some(c) = self.contracts.get_mut(contract_id) {
                    c.status = ContractStatus::Active;
                }
            }
            Event::TxApplied { tx } => self.apply_tx(tx),
            Event::TxAwaitingApproval { proposal, needs, policy_version } => {
                self.pending.insert(
                    proposal.id,
                    PendingApproval {
                        proposal: proposal.clone(),
                        needs: needs.clone(),
                        approvals: Vec::new(),
                        policy_version: *policy_version,
                        created_at: at,
                    },
                );
            }
            Event::ApprovalGranted { proposal_id, approval } => {
                if let Some(p) = self.pending.get_mut(proposal_id) {
                    p.approvals.push(approval.clone());
                }
            }
            Event::ApprovalDenied { proposal_id, reason, .. } => {
                self.resolve_pending(*proposal_id, ApprovalOutcome::Denied, reason.clone(), at);
            }
            Event::Rejected { attempt } => {
                if let Attempt::Tx { proposal } = &attempt.attempt {
                    let note = attempt.violations.iter().map(|v| v.rule_id.clone()).collect::<Vec<_>>().join(", ");
                    self.resolve_pending(proposal.id, ApprovalOutcome::Rejected, note, at);
                }
                self.rejected.push(attempt.clone());
            }
            Event::PaymentConfirmed { payment_id, bank_ref } => {
                self.payment_status.insert(*payment_id, (PaymentStatus::Confirmed, Some(bank_ref.clone())));
                if let Some(p) = self.payments.get(payment_id) {
                    if p.kind == TxKind::Deploy {
                        let tx_id = p.tx_id;
                        if let Some(c) = self.contracts.values_mut().find(|c| c.purchase_tx == Some(tx_id)) {
                            if c.status == ContractStatus::Draft {
                                c.status = ContractStatus::GoodsPurchased;
                            }
                        }
                    }
                }
            }
            Event::PaymentFailed { payment_id, reason } => {
                self.payment_status.insert(*payment_id, (PaymentStatus::Failed, Some(reason.clone())));
            }
            Event::LotsFrozen { case } => {
                for id in &case.lots {
                    if self.is_active(*id) {
                        self.lot_status.insert(*id, LotStatus::Frozen);
                    }
                }
                self.freezes.insert(case.id, case.clone());
            }
            Event::LotsUnfrozen { case_id, .. } => {
                if let Some(case) = self.freezes.get_mut(case_id) {
                    for id in &case.lots {
                        if self.lot_status.get(id) == Some(&LotStatus::Frozen) {
                            self.lot_status.insert(*id, LotStatus::Active);
                        }
                    }
                    case.closed_at = Some(at);
                }
            }
            Event::Reconciled { reconciliation } => self.reconciliations.push(reconciliation.clone()),
            Event::IncidentOpened { incident } => self.incidents.push(incident.clone()),
            Event::MerkleAnchored { anchor } => self.anchors.push(anchor.clone()),
        }
    }

    fn resolve_pending(&mut self, id: TxId, outcome: ApprovalOutcome, note: String, at: DateTime<Utc>) {
        if let Some(p) = self.pending.remove(&id) {
            self.approval_history.push(ResolvedApproval { pending: p, outcome, note, at });
        }
    }

    fn apply_tx(&mut self, tx: &AppliedTx) {
        let p = &tx.proposal;
        for id in &tx.spent {
            self.lot_status.insert(*id, LotStatus::Spent);
            self.lot_spent_by.insert(*id, p.id);
        }
        for lot in &tx.created {
            if let Some(parent) = lot.parent_id {
                self.lot_children.entry(parent).or_default().push(lot.id);
            }
            self.next_lot_seq = self.next_lot_seq.max(lot.seq + 1);
            self.lot_status.insert(lot.id, LotStatus::Active);
            self.lots.insert(lot.id, lot.clone());
        }
        for pay in &tx.payments {
            for (lot, _) in &pay.sources {
                self.lot_payments.entry(*lot).or_default().push(pay.id);
            }
            self.payment_status.insert(pay.id, (PaymentStatus::Pending, None));
            self.payments.insert(pay.id, pay.clone());
        }
        for d in &tx.positions {
            self.apply_position(d);
        }

        if let Some(cid) = p.contract_id {
            let charity: Money = sum_dest(p, |c| c == MoneyClass::Charity);
            let principal: Money = sum_dest(p, |c| c == MoneyClass::Capital);
            let markup: Money = sum_dest(p, |c| c == MoneyClass::Profit);
            if let Some(c) = self.contracts.get_mut(&cid) {
                match p.kind {
                    TxKind::Deploy => c.purchase_tx = Some(p.id),
                    TxKind::Installment => {
                        c.paid_principal += principal;
                        c.paid_markup += markup;
                        c.late_fees += charity;
                        if c.paid_total() >= c.price() {
                            c.status = ContractStatus::Repaid;
                        }
                    }
                    TxKind::LossAllocation => {
                        c.status = ContractStatus::Defaulted;
                        c.fault = p.fault;
                        if p.fault == Some(Fault::Mudarib) {
                            let loss: Money = p.write_offs.iter().map(|s| s.amount).sum();
                            c.mudarib_liability = loss - principal;
                        }
                    }
                    _ => {}
                }
            }
        }
        if p.kind == TxKind::Compensation {
            if let Some(pay) = p.refund_of.and_then(|id| self.payments.get(&id)) {
                if pay.kind == TxKind::Deploy {
                    let tx_id = pay.tx_id;
                    if let Some(c) = self.contracts.values_mut().find(|c| c.purchase_tx == Some(tx_id)) {
                        c.purchase_tx = None;
                        c.status = ContractStatus::Draft;
                    }
                }
            }
        }

        self.resolve_pending(p.id, ApprovalOutcome::Executed, String::new(), tx.at);
        self.tx_order.push(p.id);
        self.txs.insert(p.id, tx.clone());
    }

    fn apply_position(&mut self, d: &PositionDelta) {
        let (cid, inv) = match d {
            PositionDelta::Open { contract_id, investor, .. }
            | PositionDelta::Cancel { contract_id, investor, .. }
            | PositionDelta::Repay { contract_id, investor, .. }
            | PositionDelta::WriteOff { contract_id, investor, .. }
            | PositionDelta::Cover { contract_id, investor, .. } => (*contract_id, *investor),
        };
        let pos = self.positions.entry((cid, inv)).or_insert(Position {
            contract_id: cid,
            investor: inv,
            principal: Money::ZERO,
            repaid: Money::ZERO,
            written_off: Money::ZERO,
            covered: Money::ZERO,
        });
        match d {
            PositionDelta::Open { amount, .. } => pos.principal += *amount,
            PositionDelta::Cancel { amount, .. } => pos.principal -= *amount,
            PositionDelta::Repay { amount, .. } => pos.repaid += *amount,
            PositionDelta::WriteOff { amount, .. } => pos.written_off += *amount,
            PositionDelta::Cover { amount, .. } => pos.covered += *amount,
        }
        if pos.principal.is_zero() && pos.repaid.is_zero() && pos.written_off.is_zero() {
            self.positions.remove(&(cid, inv));
        }
    }
}

fn sum_dest(p: &TxProposal, f: impl Fn(MoneyClass) -> bool) -> Money {
    p.legs
        .iter()
        .filter(|l| matches!(l.dest, Dest::Lot { class, .. } if f(class)))
        .map(|l| l.amount)
        .sum()
}
