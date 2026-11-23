//! Engine: buyruqlarni tekshirib hodisalarga aylantiradi (sof funksiyalar), so'ng hodisalarni
//! hash chain'ga bog'lab holatga qo'llaydi.
//!
//! Ish tartibi: `decision = engine.xxx(...)` → `entries = engine.prepare(&decision.events, now)` →
//! entries'ni store'ga yozish (serializable tranzaksiya) → `engine.apply_entries(&entries)`.
//! Store'ga yozilmagan hodisa holatga hech qachon qo'llanmaydi.

use crate::chain::{self, decode_hash, make_entry, verify_chain, ChainError, LedgerEntry};
use crate::event::Event;
use crate::model::*;
use crate::money::Money;
use crate::policy::config::Policy;
use crate::policy::{self, rules, PolicyRule, RuleCtx};
use crate::state::State;
use crate::{local_date, trace};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Outcome {
    Executed { tx_id: TxId, payments: Vec<PaymentId> },
    PendingApproval { proposal_id: TxId, needs: Vec<NeedsApproval> },
    Rejected { violations: Vec<Violation> },
    Recorded,
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub events: Vec<Event>,
    pub outcome: Outcome,
}

impl Decision {
    fn recorded(events: Vec<Event>) -> Self {
        Decision { events, outcome: Outcome::Recorded }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("topilmadi: {0}")]
    NotFound(String),
    #[error("ziddiyat: {0}")]
    Conflict(String),
    #[error("noto'g'ri so'rov: {0}")]
    Invalid(String),
    #[error("ruxsat yo'q: {0}")]
    Forbidden(String),
}

pub type CmdResult = Result<Decision, CommandError>;

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error(transparent)]
    Chain(#[from] ChainError),
    #[error("seq {seq}: hodisani o'qib bo'lmadi: {error}")]
    Payload { seq: u64, error: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewContract {
    pub merchant: PartyId,
    pub customer: PartyId,
    pub goods: String,
    pub cost: Money,
    pub markup: Money,
    pub installments: u32,
    pub first_due: NaiveDate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnchorCheck {
    pub anchor: Anchor,
    pub recomputed_root: String,
    pub ok: bool,
}

pub struct Engine {
    state: State,
    rules: Vec<Box<dyn PolicyRule>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Engine { state: State::default(), rules: policy::default_rules() }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// Zanjirni tekshiradi va holatni qayta quradi.
    pub fn replay(entries: &[LedgerEntry]) -> Result<Self, ReplayError> {
        verify_chain(entries)?;
        let mut e = Engine::new();
        for entry in entries {
            let ev = entry.event().map_err(|err| ReplayError::Payload { seq: entry.seq, error: err.to_string() })?;
            e.state.apply(entry.at, &ev);
            e.state.head_seq = entry.seq;
            e.state.head_hash = decode_hash(&entry.hash).expect("verified");
        }
        Ok(e)
    }

    /// Hodisalarni zanjir yozuvlariga aylantiradi (holatni o'zgartirmaydi).
    pub fn prepare(&self, events: &[Event], at: DateTime<Utc>) -> Vec<LedgerEntry> {
        let mut prev = self.state.head_hash;
        let mut seq = self.state.head_seq;
        events
            .iter()
            .map(|ev| {
                seq += 1;
                let e = make_entry(&prev, seq, at, ev);
                prev = decode_hash(&e.hash).expect("own hash");
                e
            })
            .collect()
    }

    /// Store'ga yozilgan yozuvlarni holatga qo'llaydi.
    pub fn apply_entries(&mut self, entries: &[LedgerEntry]) {
        for e in entries {
            assert_eq!(e.seq, self.state.head_seq + 1, "entries must continue the chain");
            assert_eq!(decode_hash(&e.prev_hash), Some(self.state.head_hash), "entries must continue the chain");
            let ev = e.event().expect("own payload");
            self.state.apply(e.at, &ev);
            self.state.head_seq = e.seq;
            self.state.head_hash = decode_hash(&e.hash).expect("own hash");
        }
    }

    /// Test va xotiradagi rejim uchun: prepare + apply.
    pub fn commit(&mut self, events: &[Event], at: DateTime<Utc>) -> Vec<LedgerEntry> {
        let entries = self.prepare(events, at);
        self.apply_entries(&entries);
        entries
    }

    fn ctx<'a>(&'a self, now: DateTime<Utc>, approvals: &'a [Approval]) -> RuleCtx<'a> {
        RuleCtx { state: &self.state, policy: self.state.policy(), now, approvals }
    }

    /// Rad etilgan urinishni log'ga yozish (u ham signal).
    pub fn reject(&self, attempt: Attempt, violations: Vec<Violation>, now: DateTime<Utc>) -> Decision {
        let ra = RejectedAttempt { id: Uuid::new_v4(), attempt, violations: violations.clone(), at: now };
        Decision { events: vec![Event::Rejected { attempt: ra }], outcome: Outcome::Rejected { violations } }
    }

    // ------------------------------------------------------------------ ro'yxatlar va policy

    pub fn register_party(&self, party: Party, by: Actor) -> CmdResult {
        if !matches!(by, Actor::Operator | Actor::Admin) {
            return Err(CommandError::Forbidden("ishtirokchini operator yoki admin ro'yxatga oladi".into()));
        }
        if self.state.parties.contains_key(&party.id) {
            return Err(CommandError::Conflict(format!("ishtirokchi {} allaqachon bor", party.id)));
        }
        if party.name.trim().is_empty() {
            return Err(CommandError::Invalid("ism bo'sh".into()));
        }
        Ok(Decision::recorded(vec![Event::PartyRegistered { party, by }]))
    }

    pub fn propose_policy(&self, policy: Policy, by: Actor, now: DateTime<Utc>) -> CmdResult {
        if by != Actor::Operator {
            return Err(CommandError::Forbidden("policy versiyasini operator taklif qiladi".into()));
        }
        if let Some(max) = self.state.policies.keys().max() {
            if policy.version <= *max {
                return Err(CommandError::Conflict(format!("versiya {} dan katta bo'lishi kerak", max)));
            }
        }
        for id in policy.related_parties.iter().chain(policy.approved_charities.iter()) {
            if self.state.party(*id).is_none() {
                return Err(CommandError::Invalid(format!("ishtirokchi {id} ro'yxatda yo'q")));
            }
        }
        for id in &policy.approved_charities {
            if !self.state.is_party(*id, PartyKind::Charity) {
                return Err(CommandError::Invalid(format!("{id} xayriya tashkiloti emas")));
            }
        }
        if let Err(v) = policy.validate() {
            return Ok(self.reject(Attempt::Policy { version: policy.version, by }, v, now));
        }
        Ok(Decision::recorded(vec![Event::PolicyProposed { policy, by }]))
    }

    pub fn activate_policy(&self, version: u32, approval: Approval) -> CmdResult {
        if approval.role != Role::ShariaBoard {
            return Err(CommandError::Forbidden("policy'ni faqat Sharia board imzosi faollashtiradi".into()));
        }
        let Some(rec) = self.state.policies.get(&version) else {
            return Err(CommandError::NotFound(format!("policy v{version}")));
        };
        if rec.activation.is_some() {
            return Err(CommandError::Conflict(format!("policy v{version} allaqachon imzolangan")));
        }
        if let Some(active) = self.state.active_policy {
            if version < active {
                return Err(CommandError::Conflict("eski versiyani faollashtirib bo'lmaydi".into()));
            }
        }
        Ok(Decision::recorded(vec![Event::PolicyActivated { version, approval }]))
    }

    // ------------------------------------------------------------------ shartnomalar

    pub fn create_contract(&self, c: NewContract, by: Actor, now: DateTime<Utc>) -> CmdResult {
        if by != Actor::Operator {
            return Err(CommandError::Forbidden("shartnomani operator tuzadi".into()));
        }
        if !self.state.is_party(c.merchant, PartyKind::Merchant) {
            return Err(CommandError::Invalid("merchant ro'yxatda yo'q".into()));
        }
        if !self.state.is_party(c.customer, PartyKind::Customer) {
            return Err(CommandError::Invalid("mijoz ro'yxatda yo'q".into()));
        }
        if !c.cost.is_positive() || c.markup.0 < 0 {
            return Err(CommandError::Invalid("tannarx musbat, ustama manfiy bo'lmasligi kerak".into()));
        }
        if c.installments == 0 || c.installments > 120 {
            return Err(CommandError::Invalid("to'lovlar soni 1..120".into()));
        }
        let contract = Contract {
            id: Uuid::new_v4(),
            merchant: c.merchant,
            customer: c.customer,
            goods: c.goods,
            cost: c.cost,
            markup: c.markup,
            schedule: crate::contract::build_schedule(c.cost, c.markup, c.installments, c.first_due),
            status: ContractStatus::Draft,
            ownership_doc: None,
            purchase_tx: None,
            paid_principal: Money::ZERO,
            paid_markup: Money::ZERO,
            late_fees: Money::ZERO,
            fault: None,
            mudarib_liability: Money::ZERO,
            created_at: now,
        };
        Ok(Decision::recorded(vec![Event::ContractCreated { contract, by }]))
    }

    pub fn attach_ownership_doc(&self, contract_id: ContractId, doc: OwnershipDoc, by: Actor) -> CmdResult {
        if by != Actor::Operator {
            return Err(CommandError::Forbidden("hujjatni operator biriktiradi".into()));
        }
        let Some(c) = self.state.contract(contract_id) else {
            return Err(CommandError::NotFound(format!("shartnoma {contract_id}")));
        };
        if !matches!(c.status, ContractStatus::Draft | ContractStatus::GoodsPurchased) {
            return Err(CommandError::Conflict("shartnoma allaqachon faol".into()));
        }
        if doc.doc_no.trim().is_empty() || doc.sha256.len() != 64 {
            return Err(CommandError::Invalid("hujjat raqami va SHA-256 (64 hex) kerak".into()));
        }
        Ok(Decision::recorded(vec![Event::OwnershipDocAttached { contract_id, doc, by }]))
    }

    /// Mijozga sotish (Active). I4: xarid tasdiqlangan va egalik hujjati bor bo'lishi shart.
    pub fn activate_contract(&self, contract_id: ContractId, by: Actor, now: DateTime<Utc>) -> CmdResult {
        if by != Actor::Operator {
            return Err(CommandError::Forbidden("shartnomani operator faollashtiradi".into()));
        }
        let Some(c) = self.state.contract(contract_id) else {
            return Err(CommandError::NotFound(format!("shartnoma {contract_id}")));
        };
        match rules::check_activation(c) {
            Ok(()) => Ok(Decision::recorded(vec![Event::ContractActivated { contract_id, by }])),
            Err(v) => Ok(self.reject(Attempt::ActivateContract { contract_id, by }, vec![v], now)),
        }
    }

    // ------------------------------------------------------------------ tranzaksiyalar

    pub fn propose_tx(&self, p: TxProposal, now: DateTime<Utc>) -> CmdResult {
        if self.state.txs.contains_key(&p.id) || self.state.pending.contains_key(&p.id) {
            return Err(CommandError::Conflict(format!("tx {} allaqachon bor", p.id)));
        }
        if matches!(p.kind, TxKind::Deposit | TxKind::Installment) {
            if let Some(r) = &p.bank_ref {
                let dup = self.state.txs.values().any(|t| {
                    matches!(t.proposal.kind, TxKind::Deposit | TxKind::Installment) && t.proposal.bank_ref.as_ref() == Some(r)
                });
                if dup {
                    return Err(CommandError::Conflict(format!("bank_ref {r} allaqachon qayd etilgan")));
                }
            }
        }
        Ok(self.evaluate_and_decide(p, Vec::new(), now))
    }

    fn evaluate_and_decide(&self, p: TxProposal, approvals: Vec<Approval>, now: DateTime<Utc>) -> Decision {
        let ev = policy::evaluate(&self.rules, &p, &self.ctx(now, &approvals));
        if !ev.violations.is_empty() {
            return self.reject(Attempt::Tx { proposal: p }, ev.violations, now);
        }
        if !ev.needs.is_empty() {
            let id = p.id;
            let needs = ev.needs.clone();
            return Decision {
                events: vec![Event::TxAwaitingApproval { proposal: p, needs: ev.needs, policy_version: self.state.active_policy }],
                outcome: Outcome::PendingApproval { proposal_id: id, needs },
            };
        }
        let tx = compute_effects(&p, &self.state, self.state.active_policy, approvals, now);
        let outcome = Outcome::Executed { tx_id: p.id, payments: tx.payments.iter().map(|x| x.id).collect() };
        Decision { events: vec![Event::TxApplied { tx }], outcome }
    }

    /// Imzo qo'shish. Kerakli barcha imzolar yig'ilsa, tx joriy holatga qayta tekshiriladi va bajariladi.
    pub fn approve(&self, proposal_id: TxId, approval: Approval, now: DateTime<Utc>) -> CmdResult {
        let Some(pending) = self.state.pending.get(&proposal_id) else {
            return Err(CommandError::NotFound(format!("tasdiq kutayotgan tx {proposal_id}")));
        };
        if !pending.missing_roles().contains(&approval.role) {
            return Err(CommandError::Forbidden(format!("{:?} imzosi bu tx uchun talab qilinmaydi", approval.role)));
        }
        let mut approvals = pending.approvals.clone();
        approvals.push(approval.clone());
        let mut events = vec![Event::ApprovalGranted { proposal_id, approval }];
        let still_missing: Vec<Role> =
            pending.missing_roles().into_iter().filter(|r| !approvals.iter().any(|a| a.role == *r)).collect();
        if !still_missing.is_empty() {
            return Ok(Decision {
                events,
                outcome: Outcome::PendingApproval { proposal_id, needs: pending.needs.clone() },
            });
        }
        let d = self.evaluate_and_decide(pending.proposal.clone(), approvals, now);
        events.extend(d.events);
        Ok(Decision { events, outcome: d.outcome })
    }

    pub fn deny(&self, proposal_id: TxId, by: Actor, reason: String, now: DateTime<Utc>) -> CmdResult {
        let Some(pending) = self.state.pending.get(&proposal_id) else {
            return Err(CommandError::NotFound(format!("tasdiq kutayotgan tx {proposal_id}")));
        };
        if !pending.missing_roles().contains(&by.role()) {
            return Err(CommandError::Forbidden("bu tx bo'yicha qaror sizning rolingizga tegishli emas".into()));
        }
        let violations: Vec<Violation> = pending
            .needs
            .iter()
            .map(|n| Violation { rule_id: n.rule_id.clone(), reason: format!("{:?} rad etdi: {reason}", by.role()) })
            .collect();
        let mut events = vec![Event::ApprovalDenied { proposal_id, by, reason }];
        let d = self.reject(Attempt::Tx { proposal: pending.proposal.clone() }, violations, now);
        events.extend(d.events);
        Ok(Decision { events, outcome: d.outcome })
    }

    // ------------------------------------------------------------------ bank

    pub fn payment_confirmed(&self, payment_id: PaymentId, bank_ref: String) -> CmdResult {
        if !self.state.payments.contains_key(&payment_id) {
            return Err(CommandError::NotFound(format!("to'lov {payment_id}")));
        }
        if self.state.payment_status(payment_id) != PaymentStatus::Pending {
            return Err(CommandError::Conflict("to'lov holati allaqachon yakunlangan".into()));
        }
        Ok(Decision::recorded(vec![Event::PaymentConfirmed { payment_id, bank_ref }]))
    }

    /// Bank rad etdi: `PaymentFailed` va pulni asl lotlar egasiga qaytaruvchi `Compensation` tx.
    pub fn payment_failed(&self, payment_id: PaymentId, reason: String, now: DateTime<Utc>) -> CmdResult {
        let Some(pay) = self.state.payments.get(&payment_id) else {
            return Err(CommandError::NotFound(format!("to'lov {payment_id}")));
        };
        if self.state.payment_status(payment_id) != PaymentStatus::Pending {
            return Err(CommandError::Conflict("to'lov holati allaqachon yakunlangan".into()));
        }
        let mut comp = TxProposal::new(TxKind::Compensation, Actor::System, format!("Bank rad etdi: {reason}"));
        comp.refund_of = Some(payment_id);
        comp.bank_ref = Some(format!("refund:{payment_id}"));
        for (lot_id, amount) in &pay.sources {
            let lot = self.state.lot(*lot_id).expect("payment source lot exists");
            comp.legs.push(Leg {
                source: Source::Refund { lot_id: *lot_id },
                dest: Dest::Lot { class: lot.class, owner: lot.owner },
                amount: *amount,
            });
        }
        let d = self.evaluate_and_decide(comp, Vec::new(), now);
        let mut events = vec![Event::PaymentFailed { payment_id, reason }];
        events.extend(d.events);
        Ok(Decision { events, outcome: d.outcome })
    }

    /// I11: ledger bo'yicha kutilgan qoldiq bank qoldig'iga teng bo'lishi kerak.
    pub fn reconcile(&self, bank_balance: Money, now: DateTime<Utc>) -> Decision {
        let ledger_balance = self.state.expected_bank_balance();
        let ok = ledger_balance == bank_balance;
        let mut events = vec![Event::Reconciled {
            reconciliation: Reconciliation { id: Uuid::new_v4(), ledger_balance, bank_balance, ok, at: now },
        }];
        if !ok {
            events.push(Event::IncidentOpened {
                incident: Incident {
                    id: Uuid::new_v4(),
                    kind: IncidentKind::ReconciliationMismatch,
                    details: format!(
                        "I11: ledger {ledger_balance} ≠ bank {bank_balance} (farq {})",
                        bank_balance - ledger_balance
                    ),
                    at: now,
                },
            });
        }
        Decision::recorded(events)
    }

    pub fn open_incident(&self, kind: IncidentKind, details: String, now: DateTime<Utc>) -> Decision {
        Decision::recorded(vec![Event::IncidentOpened { incident: Incident { id: Uuid::new_v4(), kind, details, at: now } }])
    }

    // ------------------------------------------------------------------ tracing & freeze

    pub fn freeze(
        &self,
        root_lot: LotId,
        hop_limit: u32,
        window_days: u32,
        reason: String,
        by: Actor,
        now: DateTime<Utc>,
    ) -> CmdResult {
        if !matches!(by, Actor::Admin | Actor::ShariaBoard) {
            return Err(CommandError::Forbidden("muzlatish — admin yoki Sharia board vakolati".into()));
        }
        let Some(root) = self.state.lot(root_lot) else {
            return Err(CommandError::NotFound(format!("lot {root_lot}")));
        };
        let nodes = trace::forward(&self.state, root_lot, hop_limit, window_days);
        let mut lots = Vec::new();
        let mut flagged = Vec::new();
        for n in nodes {
            if let trace::TraceNode::Lot { lot_id, .. } = n {
                let l = self.state.lot(lot_id).expect("traced lot");
                let suspect_own = l.owner == root.owner && root.owner.is_some();
                if suspect_own && self.state.is_active(lot_id) {
                    lots.push(lot_id);
                } else if self.state.is_active(lot_id) {
                    // Vijdonli oluvchi: boshqa egalar va fond klasslari muzlatilmaydi, faqat belgilanadi.
                    flagged.push(lot_id);
                }
            }
        }
        if lots.is_empty() {
            return Err(CommandError::Invalid("muzlatiladigan faol lot topilmadi".into()));
        }
        let case = FreezeCase {
            id: Uuid::new_v4(),
            root_lot,
            lots,
            flagged,
            reason,
            hop_limit,
            window_days,
            opened_by: by,
            opened_at: now,
            closed_at: None,
        };
        Ok(Decision::recorded(vec![Event::LotsFrozen { case }]))
    }

    pub fn unfreeze(&self, case_id: CaseId, by: Actor, reason: String) -> CmdResult {
        if !matches!(by, Actor::Admin | Actor::ShariaBoard) {
            return Err(CommandError::Forbidden("muzlatishni bekor qilish — admin yoki Sharia board vakolati".into()));
        }
        match self.state.freezes.get(&case_id) {
            None => Err(CommandError::NotFound(format!("ish {case_id}"))),
            Some(c) if c.closed_at.is_some() => Err(CommandError::Conflict("ish allaqachon yopilgan".into())),
            Some(_) => Ok(Decision::recorded(vec![Event::LotsUnfrozen { case_id, by, reason }])),
        }
    }

    // ------------------------------------------------------------------ anchoring

    /// Kunlik Merkle root. `sign` — Omonat kaliti (HSM) bilan imzolaydi: (signature_hex, public_key_hex).
    pub fn anchor(
        &self,
        day: NaiveDate,
        entries: &[LedgerEntry],
        sign: impl Fn(&[u8]) -> (String, String),
        now: DateTime<Utc>,
    ) -> CmdResult {
        if self.state.anchors.iter().any(|a| a.day == day) {
            return Err(CommandError::Conflict(format!("{day} uchun root allaqachon e'lon qilingan")));
        }
        let day_entries: Vec<&LedgerEntry> = entries.iter().filter(|e| local_date(e.at) == day).collect();
        let (Some(first), Some(last)) = (day_entries.first(), day_entries.last()) else {
            return Err(CommandError::Invalid(format!("{day} kuni yozuv yo'q")));
        };
        let root = chain::merkle_root_of(entries, first.seq, last.seq);
        let (signature, public_key) = sign(&anchor_message(day, first.seq, last.seq, &root));
        Ok(Decision::recorded(vec![Event::MerkleAnchored {
            anchor: Anchor { day, from_seq: first.seq, to_seq: last.seq, root, signature, public_key, at: now },
        }]))
    }
}

/// Anchor imzosi ostidagi xabar.
pub fn anchor_message(day: NaiveDate, from_seq: u64, to_seq: u64, root: &str) -> Vec<u8> {
    format!("omonat.anchor.v1|{day}|{from_seq}|{to_seq}|{root}").into_bytes()
}

/// E'lon qilingan root'larni store'dagi yozuvlardan qayta hisoblab solishtiradi.
pub fn verify_anchors(entries: &[LedgerEntry], anchors: &[Anchor]) -> Vec<AnchorCheck> {
    anchors
        .iter()
        .map(|a| {
            let recomputed_root = chain::merkle_root_of(entries, a.from_seq, a.to_seq);
            AnchorCheck { ok: recomputed_root == a.root, anchor: a.clone(), recomputed_root }
        })
        .collect()
}

fn per_owner_dest(p: &TxProposal, class: MoneyClass) -> BTreeMap<PartyId, Money> {
    let mut m = BTreeMap::new();
    for l in &p.legs {
        if let Dest::Lot { class: c, owner: Some(o) } = l.dest {
            if c == class {
                *m.entry(o).or_insert(Money::ZERO) += l.amount;
            }
        }
    }
    m
}

/// Policy'dan o'tgan taklifning ta'sirlari: sarflangan lotlar, yangi lotlar (change bilan),
/// tashqi to'lovlar va pozitsiya o'zgarishlari. UTXO uslubi: parent `Spent`, qoldiq egasiga yangi lot.
fn compute_effects(
    p: &TxProposal,
    s: &State,
    policy_version: Option<u32>,
    approvals: Vec<Approval>,
    at: DateTime<Utc>,
) -> AppliedTx {
    let mut seq = s.next_lot_seq.max(1);
    let mut created = Vec::new();
    let mut spent: Vec<LotId> = Vec::new();
    let mut drawn: BTreeMap<LotId, Money> = BTreeMap::new();
    let mut payments: Vec<Payment> = Vec::new();

    for leg in &p.legs {
        let parent = match leg.source {
            Source::Lot { lot_id } => {
                if !spent.contains(&lot_id) {
                    spent.push(lot_id);
                }
                *drawn.entry(lot_id).or_insert(Money::ZERO) += leg.amount;
                Some(lot_id)
            }
            Source::Refund { lot_id } => Some(lot_id),
            Source::Inflow => None,
        };
        let parent_lot = parent.and_then(|id| s.lot(id));
        match leg.dest {
            Dest::Lot { class, owner } => {
                created.push(Lot {
                    id: Uuid::new_v4(),
                    owner,
                    class,
                    amount: leg.amount,
                    seq,
                    tx_id: p.id,
                    parent_id: parent,
                    contract_id: p.contract_id.or(parent_lot.and_then(|l| l.contract_id)),
                    created_at: at,
                });
                seq += 1;
            }
            Dest::External { party } => {
                let src = parent.expect("FLOW: external legs always spend a lot");
                match payments.iter_mut().find(|x| x.beneficiary == party) {
                    Some(pay) => {
                        pay.amount += leg.amount;
                        pay.sources.push((src, leg.amount));
                    }
                    None => payments.push(Payment {
                        id: Uuid::new_v4(),
                        tx_id: p.id,
                        kind: p.kind,
                        beneficiary: party,
                        amount: leg.amount,
                        sources: vec![(src, leg.amount)],
                        purpose: p.purpose.clone(),
                        created_at: at,
                    }),
                }
            }
        }
    }
    for id in &spent {
        let lot = s.lot(*id).expect("validated");
        let change = lot.amount - drawn[id];
        if change.is_positive() {
            created.push(Lot {
                id: Uuid::new_v4(),
                owner: lot.owner,
                class: lot.class,
                amount: change,
                seq,
                tx_id: p.id,
                parent_id: Some(lot.id),
                contract_id: lot.contract_id,
                created_at: at,
            });
            seq += 1;
        }
    }

    let mut positions = Vec::new();
    match (p.kind, p.contract_id) {
        (TxKind::Deploy, Some(cid)) => {
            let mut per: BTreeMap<PartyId, Money> = BTreeMap::new();
            for leg in &p.legs {
                if let Source::Lot { lot_id } = leg.source {
                    if let Some(o) = s.lot(lot_id).and_then(|l| l.owner) {
                        *per.entry(o).or_insert(Money::ZERO) += leg.amount;
                    }
                }
            }
            for (investor, amount) in per {
                positions.push(PositionDelta::Open { contract_id: cid, investor, amount });
            }
        }
        (TxKind::Installment, Some(cid)) => {
            for (investor, amount) in per_owner_dest(p, MoneyClass::Capital) {
                positions.push(PositionDelta::Repay { contract_id: cid, investor, amount });
            }
        }
        (TxKind::LossAllocation, Some(cid)) => {
            for w in &p.write_offs {
                positions.push(PositionDelta::WriteOff { contract_id: cid, investor: w.investor, amount: w.amount });
            }
            for (investor, amount) in per_owner_dest(p, MoneyClass::Capital) {
                positions.push(PositionDelta::Cover { contract_id: cid, investor, amount });
            }
        }
        (TxKind::ReserveUse, Some(cid)) => {
            for (investor, amount) in per_owner_dest(p, MoneyClass::Capital) {
                positions.push(PositionDelta::Cover { contract_id: cid, investor, amount });
            }
        }
        (TxKind::Compensation, _) => {
            if let Some(pay) = p.refund_of.and_then(|id| s.payments.get(&id)) {
                if pay.kind == TxKind::Deploy {
                    if let Some(orig) = s.txs.get(&pay.tx_id) {
                        for d in &orig.positions {
                            if let PositionDelta::Open { contract_id, investor, amount } = d {
                                positions.push(PositionDelta::Cancel {
                                    contract_id: *contract_id,
                                    investor: *investor,
                                    amount: *amount,
                                });
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    AppliedTx { proposal: p.clone(), policy_version, approvals, spent, created, payments, positions, at }
}
