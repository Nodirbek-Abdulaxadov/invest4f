//! Read side: investor, auditor, regulyator va operator ko'rinishlari.

use crate::model::*;
use crate::money::Money;
use crate::policy::config::PolicyRecord;
use crate::state::{PendingApproval, ResolvedApproval, State};
use crate::trace;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

pub const NOT_GUARANTEED: &str = "kafolatlanmagan";

#[derive(Clone, Debug, Serialize)]
pub struct PositionView {
    pub contract_id: ContractId,
    pub goods: String,
    pub status: ContractStatus,
    pub principal: Money,
    pub repaid: Money,
    pub outstanding: Money,
    pub written_off: Money,
    pub covered: Money,
    /// Shartnomadagi ulush (bps).
    pub share_bps: u32,
    pub expected_profit: Money,
    pub expected_profit_label: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct DividendSource {
    pub amount: Money,
    pub contract_id: Option<ContractId>,
    pub goods: Option<String>,
    /// Ustama qaysi oyda tushgan (YYYY-MM).
    pub month: Option<String>,
    pub installment_tx: Option<TxId>,
    pub lineage: Vec<trace::LineageStep>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DividendView {
    pub payment_id: PaymentId,
    pub amount: Money,
    pub status: PaymentStatus,
    pub at: DateTime<Utc>,
    pub sources: Vec<DividendSource>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InvestorDashboard {
    pub investor: Party,
    pub deposited: Money,
    pub liquid: Money,
    pub active: Money,
    pub returned_principal: Money,
    pub withdrawn: Money,
    pub profit_credited: Money,
    pub profit_available: Money,
    pub dividends_paid: Money,
    pub expected_profit: Money,
    pub expected_profit_label: &'static str,
    pub overdue: Money,
    pub overdue_contracts: usize,
    pub defaulted_loss: Money,
    pub covered_loss: Money,
    pub frozen: Money,
    pub positions: Vec<PositionView>,
    pub dividends: Vec<DividendView>,
    pub withdrawable_now: Money,
}

fn share_bps(part: Money, whole: Money) -> u32 {
    if whole.is_positive() {
        ((part.0 as i128 * 10_000) / whole.0 as i128) as u32
    } else {
        0
    }
}

fn not_failed(s: &State, p: &Payment) -> bool {
    s.payment_status(p.id) != PaymentStatus::Failed
}

pub fn investor_dashboard(s: &State, investor: PartyId, today: NaiveDate) -> Option<InvestorDashboard> {
    let party = s.party(investor)?.clone();
    if party.kind != PartyKind::Investor {
        return None;
    }
    let mut deposited = Money::ZERO;
    let mut profit_credited = Money::ZERO;
    for tx in s.txs.values() {
        for lot in &tx.created {
            if lot.owner == Some(investor) && lot.parent_id.is_none() && tx.proposal.kind == TxKind::Deposit {
                deposited += lot.amount;
            }
            if lot.owner == Some(investor) && lot.class == MoneyClass::InvestorProfit && tx.proposal.kind == TxKind::ProfitSplit {
                profit_credited += lot.amount;
            }
        }
    }
    let my_payments: Vec<&Payment> =
        s.payments.values().filter(|p| p.beneficiary == investor && not_failed(s, p)).collect();
    let withdrawn = my_payments.iter().filter(|p| p.kind == TxKind::Withdrawal).map(|p| p.amount).sum();
    let dividends_paid = my_payments.iter().filter(|p| p.kind == TxKind::Dividend).map(|p| p.amount).sum();

    let mut positions = Vec::new();
    let mut active = Money::ZERO;
    let mut returned = Money::ZERO;
    let mut overdue = Money::ZERO;
    let mut overdue_contracts = 0;
    let mut defaulted_loss = Money::ZERO;
    let mut covered_loss = Money::ZERO;
    let mut expected_total = Money::ZERO;
    for pos in s.investor_positions(investor) {
        let Some(c) = s.contract(pos.contract_id) else { continue };
        let status = c.effective_status(today);
        let expected = if matches!(status, ContractStatus::Active | ContractStatus::Overdue | ContractStatus::GoodsPurchased | ContractStatus::Draft) {
            c.remaining_markup().mul_div_floor(pos.principal, c.cost)
        } else {
            Money::ZERO
        };
        expected_total += expected;
        returned += pos.repaid;
        defaulted_loss += pos.uncovered_loss();
        covered_loss += pos.covered;
        if status != ContractStatus::Defaulted {
            active += pos.outstanding();
        }
        if status == ContractStatus::Overdue {
            overdue += pos.outstanding();
            overdue_contracts += 1;
        }
        positions.push(PositionView {
            contract_id: c.id,
            goods: c.goods.clone(),
            status,
            principal: pos.principal,
            repaid: pos.repaid,
            outstanding: pos.outstanding(),
            written_off: pos.written_off,
            covered: pos.covered,
            share_bps: share_bps(pos.principal, c.cost),
            expected_profit: expected,
            expected_profit_label: NOT_GUARANTEED,
        });
    }

    let mut dividends: Vec<DividendView> = s
        .payments
        .values()
        .filter(|p| p.beneficiary == investor && p.kind == TxKind::Dividend)
        .map(|p| DividendView {
            payment_id: p.id,
            amount: p.amount,
            status: s.payment_status(p.id),
            at: p.created_at,
            sources: p
                .sources
                .iter()
                .map(|(lot, amount)| {
                    let lineage = trace::lineage(s, *lot);
                    let root = lineage.iter().find(|st| st.class == MoneyClass::Profit && st.tx_kind == Some(TxKind::Installment));
                    DividendSource {
                        amount: *amount,
                        contract_id: root.and_then(|r| r.contract_id),
                        goods: root.and_then(|r| r.contract_id).and_then(|c| s.contract(c)).map(|c| c.goods.clone()),
                        month: root.map(|r| crate::local_date(r.at).format("%Y-%m").to_string()),
                        installment_tx: root.map(|r| r.tx_id),
                        lineage,
                    }
                })
                .collect(),
        })
        .collect();
    dividends.sort_by_key(|d| std::cmp::Reverse(d.at));

    let liquid = s.owner_balance(investor, MoneyClass::Capital);
    Some(InvestorDashboard {
        investor: party,
        deposited,
        liquid,
        active,
        returned_principal: returned,
        withdrawn,
        profit_credited,
        profit_available: s.owner_balance(investor, MoneyClass::InvestorProfit),
        dividends_paid,
        expected_profit: expected_total,
        expected_profit_label: NOT_GUARANTEED,
        overdue,
        overdue_contracts,
        defaulted_loss,
        covered_loss,
        frozen: s.frozen_balance(Some(investor)),
        positions,
        dividends,
        withdrawable_now: liquid,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct ClassBalance {
    pub class: MoneyClass,
    pub amount: Money,
}

pub fn class_balances(s: &State) -> Vec<ClassBalance> {
    use MoneyClass::*;
    [Capital, Profit, InvestorProfit, MudaribShare, Reserve, Charity]
        .into_iter()
        .map(|class| ClassBalance { class, amount: s.class_balance(class) })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
pub struct Movement {
    pub tx_id: TxId,
    pub kind: TxKind,
    pub amount: Money,
    /// "in" — klassga kirim, "out" — klassdan chiqim.
    pub direction: &'static str,
    pub purpose: String,
    pub at: DateTime<Utc>,
}

/// Klass bo'yicha harakatlar (change lotlarisiz).
pub fn class_movements(s: &State, class: MoneyClass) -> Vec<Movement> {
    let mut out = Vec::new();
    for id in s.tx_order.iter().rev() {
        let tx = &s.txs[id];
        let p = &tx.proposal;
        let mut inflow = Money::ZERO;
        let mut outflow = Money::ZERO;
        for leg in &p.legs {
            let src_class = match &leg.source {
                Source::Lot { lot_id } | Source::Refund { lot_id } => s.lot(*lot_id).map(|l| l.class),
                Source::Inflow => None,
            };
            let dst_class = match leg.dest {
                Dest::Lot { class, .. } => Some(class),
                Dest::External { .. } => None,
            };
            if dst_class == Some(class) && src_class != Some(class) {
                inflow += leg.amount;
            }
            if src_class == Some(class) && dst_class != Some(class) && !matches!(leg.source, Source::Refund { .. }) {
                outflow += leg.amount;
            }
        }
        for (amount, direction) in [(inflow, "in"), (outflow, "out")] {
            if amount.is_positive() {
                out.push(Movement { tx_id: p.id, kind: p.kind, amount, direction, purpose: p.purpose.clone(), at: tx.at });
            }
        }
    }
    out
}

#[derive(Clone, Debug, Serialize)]
pub struct PaymentView {
    pub payment: Payment,
    pub status: PaymentStatus,
    pub bank_note: Option<String>,
    pub beneficiary_name: String,
}

pub fn payment_view(s: &State, p: &Payment) -> PaymentView {
    let (status, note) = s.payment_status.get(&p.id).cloned().unwrap_or((PaymentStatus::Pending, None));
    PaymentView {
        payment: p.clone(),
        status,
        bank_note: note,
        beneficiary_name: s.party(p.beneficiary).map(|x| x.name.clone()).unwrap_or_default(),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AuditorView {
    pub balances: Vec<ClassBalance>,
    pub expected_bank_balance: Money,
    pub pending: Vec<PendingApproval>,
    pub approval_history: Vec<ResolvedApproval>,
    pub rejected: Vec<RejectedAttempt>,
    pub reserve_movements: Vec<Movement>,
    pub charity_movements: Vec<Movement>,
    pub related_party_payments: Vec<PaymentView>,
    pub policies: Vec<PolicyRecord>,
    pub active_policy: Option<u32>,
    pub anchors: Vec<Anchor>,
    pub incidents: Vec<Incident>,
    pub freezes: Vec<FreezeCase>,
    pub head_seq: u64,
}

pub fn auditor_view(s: &State) -> AuditorView {
    let related: Vec<PartyId> = s.policies.values().flat_map(|r| r.policy.related_parties.clone()).collect();
    let mut related_party_payments: Vec<PaymentView> =
        s.payments.values().filter(|p| related.contains(&p.beneficiary)).map(|p| payment_view(s, p)).collect();
    related_party_payments.sort_by_key(|p| std::cmp::Reverse(p.payment.created_at));
    let mut rejected = s.rejected.clone();
    rejected.reverse();
    let mut history = s.approval_history.clone();
    history.reverse();
    let mut incidents = s.incidents.clone();
    incidents.reverse();
    let mut anchors = s.anchors.clone();
    anchors.reverse();
    AuditorView {
        balances: class_balances(s),
        expected_bank_balance: s.expected_bank_balance(),
        pending: s.pending.values().cloned().collect(),
        approval_history: history,
        rejected,
        reserve_movements: class_movements(s, MoneyClass::Reserve),
        charity_movements: class_movements(s, MoneyClass::Charity),
        related_party_payments,
        policies: s.policies.values().cloned().collect(),
        active_policy: s.active_policy,
        anchors,
        incidents,
        freezes: s.freezes.values().cloned().collect(),
        head_seq: s.head_seq,
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RegulatorView {
    /// Investorlar kapitali: likvid + shartnomalardagi qoldiq.
    pub aum: Money,
    pub liquidity: Money,
    pub liquidity_bps: u32,
    pub outstanding: Money,
    pub npl: Money,
    pub npl_bps: u32,
    pub written_off: Money,
    pub reserve: Money,
    pub charity: Money,
    pub investors: usize,
    pub contracts_by_status: BTreeMap<String, usize>,
    pub incidents: Vec<Incident>,
    pub reconciliations: Vec<Reconciliation>,
    pub rejected_count: usize,
    pub anchors: Vec<Anchor>,
}

pub fn regulator_view(s: &State, today: NaiveDate) -> RegulatorView {
    let liquidity = s.class_balance(MoneyClass::Capital);
    let mut outstanding = Money::ZERO;
    let mut npl = Money::ZERO;
    let mut written_off = Money::ZERO;
    let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
    for c in s.contracts.values() {
        let st = c.effective_status(today);
        *by_status.entry(format!("{st:?}")).or_default() += 1;
        for p in s.contract_positions(c.id) {
            written_off += p.uncovered_loss();
            if st != ContractStatus::Defaulted {
                outstanding += p.outstanding();
                if st == ContractStatus::Overdue {
                    npl += p.outstanding();
                }
            }
        }
    }
    let aum = liquidity + outstanding;
    let mut reconciliations = s.reconciliations.clone();
    reconciliations.reverse();
    reconciliations.truncate(30);
    let mut incidents = s.incidents.clone();
    incidents.reverse();
    let mut anchors = s.anchors.clone();
    anchors.reverse();
    RegulatorView {
        aum,
        liquidity,
        liquidity_bps: share_bps(liquidity, aum),
        outstanding,
        npl,
        npl_bps: share_bps(npl, outstanding),
        written_off,
        reserve: s.class_balance(MoneyClass::Reserve),
        charity: s.class_balance(MoneyClass::Charity),
        investors: s.parties.values().filter(|p| p.kind == PartyKind::Investor).count(),
        contracts_by_status: by_status,
        incidents,
        reconciliations,
        rejected_count: s.rejected.len(),
        anchors,
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ContractView {
    pub contract: Contract,
    pub effective_status: ContractStatus,
    pub due_until_today: Money,
    pub positions: Vec<Position>,
    pub profit_undistributed: Money,
    pub merchant_name: String,
    pub customer_name: String,
}

pub fn contract_view(s: &State, c: &Contract, today: NaiveDate) -> ContractView {
    ContractView {
        contract: c.clone(),
        effective_status: c.effective_status(today),
        due_until_today: c.due_until(today),
        positions: s.contract_positions(c.id).into_iter().cloned().collect(),
        profit_undistributed: State::sum(&s.active_lots(|l| l.class == MoneyClass::Profit && l.contract_id == Some(c.id))),
        merchant_name: s.party(c.merchant).map(|p| p.name.clone()).unwrap_or_default(),
        customer_name: s.party(c.customer).map(|p| p.name.clone()).unwrap_or_default(),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TxView {
    pub tx: AppliedTx,
    pub payments: Vec<PaymentView>,
}

pub fn recent_txs(s: &State, limit: usize) -> Vec<TxView> {
    s.tx_order
        .iter()
        .rev()
        .take(limit)
        .map(|id| {
            let tx = &s.txs[id];
            TxView { tx: tx.clone(), payments: tx.payments.iter().map(|p| payment_view(s, p)).collect() }
        })
        .collect()
}
