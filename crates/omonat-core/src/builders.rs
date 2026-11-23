//! Taklif quruvchilar va Allocation Engine. Ular faqat `TxProposal` tayyorlaydi — bajarish
//! huquqini Policy Engine beradi. Qo'lda tuzilgan (noto'g'ri) taklif ham xuddi shu tekshiruvdan o'tadi.

use crate::contract::split_payment;
use crate::model::*;
use crate::money::{split_pro_rata, Money};
use crate::state::State;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    #[error("topilmadi: {0}")]
    NotFound(String),
    #[error("mablag' yetarli emas: kerak {needed}, mavjud {available}")]
    Insufficient { needed: Money, available: Money },
    #[error("{0}")]
    Invalid(String),
}

type R<T> = Result<T, BuildError>;

/// Lotlardan ketma-ket (FIFO) olib, har bir maqsad uchun leg tuzadi.
pub fn draw(lots: &[&Lot], targets: &[(Dest, Money)]) -> R<Vec<Leg>> {
    let needed: Money = targets.iter().map(|t| t.1).sum();
    let available: Money = lots.iter().map(|l| l.amount).sum();
    if needed > available {
        return Err(BuildError::Insufficient { needed, available });
    }
    let mut legs = Vec::new();
    let mut i = 0usize;
    let mut left = lots.first().map(|l| l.amount).unwrap_or(Money::ZERO);
    for (dest, amount) in targets {
        let mut need = *amount;
        while need.is_positive() {
            if left.is_zero() {
                i += 1;
                left = lots[i].amount;
                continue;
            }
            let take = need.min(left);
            legs.push(Leg { source: Source::Lot { lot_id: lots[i].id }, dest: dest.clone(), amount: take });
            need -= take;
            left -= take;
        }
    }
    Ok(legs)
}

fn contract(s: &State, id: ContractId) -> R<&Contract> {
    s.contract(id).ok_or_else(|| BuildError::NotFound(format!("shartnoma {id}")))
}

fn capital(owner: PartyId) -> Dest {
    Dest::Lot { class: MoneyClass::Capital, owner: Some(owner) }
}

/// Bankdan kelgan investor puli → `Capital(Owner)`.
pub fn deposit(investor: PartyId, amount: Money, bank_ref: &str) -> TxProposal {
    let mut p = TxProposal::new(TxKind::Deposit, Actor::Bank, "Investor depoziti (mudaraba)");
    p.bank_ref = Some(bank_ref.to_string());
    p.legs.push(Leg { source: Source::Inflow, dest: capital(investor), amount });
    p
}

/// Mijozning muddatli to'lovi: asosiy qism pozitsiyalarga pro-rata `Capital(Owner)` ga,
/// ustama `Profit(Contract)` ga, kechikish jarimasi `Charity` ga.
pub fn installment(s: &State, contract_id: ContractId, amount: Money, late_fee: Money, bank_ref: &str) -> R<TxProposal> {
    let c = contract(s, contract_id)?;
    if late_fee.0 < 0 || late_fee > amount {
        return Err(BuildError::Invalid("jarima 0 va to'lov summasi orasida bo'lishi kerak".into()));
    }
    let applied = amount - late_fee;
    let remaining = c.price() - c.paid_total();
    if applied > remaining {
        return Err(BuildError::Invalid(format!(
            "to'lov shartnoma qoldig'idan ({remaining}) oshadi — ortiqcha summa mijozga qaytariladi"
        )));
    }
    let (principal, markup) = split_payment(c, applied);
    let weights: Vec<(PartyId, Money)> = s
        .contract_positions(contract_id)
        .iter()
        .map(|p| (p.investor, p.outstanding()))
        .filter(|w| w.1.is_positive())
        .collect();
    if principal.is_positive() && weights.is_empty() {
        return Err(BuildError::Invalid("shartnomada ochiq pozitsiya yo'q".into()));
    }
    let mut p = TxProposal::new(TxKind::Installment, Actor::Bank, format!("Mijoz to'lovi: {}", c.goods));
    p.contract_id = Some(contract_id);
    p.bank_ref = Some(bank_ref.to_string());
    for (investor, a) in split_pro_rata(principal, &weights) {
        if a.is_positive() {
            p.legs.push(Leg { source: Source::Inflow, dest: capital(investor), amount: a });
        }
    }
    if markup.is_positive() {
        p.legs.push(Leg { source: Source::Inflow, dest: Dest::Lot { class: MoneyClass::Profit, owner: None }, amount: markup });
    }
    if late_fee.is_positive() {
        p.legs.push(Leg { source: Source::Inflow, dest: Dest::Lot { class: MoneyClass::Charity, owner: None }, amount: late_fee });
    }
    Ok(p)
}

/// Allocation Engine (segregated model): bo'sh kapitalni investorlarning bo'sh kapitaliga mutanosib
/// taqsimlab, merchantga tovar xaridi uchun to'lov taklifini tuzadi. Har bir investor ichida FIFO.
pub fn deploy(s: &State, contract_id: ContractId) -> R<TxProposal> {
    let c = contract(s, contract_id)?;
    let weights: Vec<(PartyId, Money)> = s
        .parties
        .values()
        .filter(|p| p.kind == PartyKind::Investor)
        .map(|p| (p.id, s.owner_balance(p.id, MoneyClass::Capital)))
        .filter(|w| w.1.is_positive())
        .collect();
    let idle: Money = weights.iter().map(|w| w.1).sum();
    if idle < c.cost {
        return Err(BuildError::Insufficient { needed: c.cost, available: idle });
    }
    let mut p = TxProposal::new(TxKind::Deploy, Actor::Operator, format!("Tovar xaridi: {}", c.goods));
    p.contract_id = Some(contract_id);
    for (investor, a) in split_pro_rata(c.cost, &weights) {
        if a.is_positive() {
            let lots = s.owner_lots(investor, MoneyClass::Capital);
            p.legs.extend(draw(&lots, &[(Dest::External { party: c.merchant }, a)])?);
        }
    }
    Ok(p)
}

/// Foyda taqsimoti: rezerv (so'ralgan %), mudarib ulushi (policy %), qolgani investorlarga pro-rata.
pub fn profit_split(s: &State, contract_id: ContractId, reserve_bps: u32) -> R<TxProposal> {
    contract(s, contract_id)?;
    let pol = s.policy().ok_or_else(|| BuildError::Invalid("faol policy yo'q".into()))?;
    let lots = s.active_lots(|l| l.class == MoneyClass::Profit && l.contract_id == Some(contract_id));
    let profit = State::sum(&lots);
    if !profit.is_positive() {
        return Err(BuildError::Invalid("taqsimlanadigan foyda yo'q".into()));
    }
    let reserve = profit.mul_bps_floor(reserve_bps);
    let mudarib = (profit - reserve).mul_bps_floor(pol.mudarib_share_bps);
    let investors = profit - reserve - mudarib;
    let weights: Vec<(PartyId, Money)> = s
        .contract_positions(contract_id)
        .iter()
        .map(|p| (p.investor, p.principal))
        .filter(|w| w.1.is_positive())
        .collect();
    let mut targets = vec![
        (Dest::Lot { class: MoneyClass::Reserve, owner: None }, reserve),
        (Dest::Lot { class: MoneyClass::MudaribShare, owner: None }, mudarib),
    ];
    for (investor, a) in split_pro_rata(investors, &weights) {
        targets.push((Dest::Lot { class: MoneyClass::InvestorProfit, owner: Some(investor) }, a));
    }
    let mut p = TxProposal::new(TxKind::ProfitSplit, Actor::Operator, "Foyda taqsimoti");
    p.contract_id = Some(contract_id);
    p.legs = draw(&lots, &targets)?;
    Ok(p)
}

pub fn dividend(s: &State, investor: PartyId, amount: Option<Money>) -> R<TxProposal> {
    let lots = s.owner_lots(investor, MoneyClass::InvestorProfit);
    let amount = amount.unwrap_or_else(|| State::sum(&lots));
    if !amount.is_positive() {
        return Err(BuildError::Invalid("to'lanadigan foyda yo'q".into()));
    }
    let mut p = TxProposal::new(TxKind::Dividend, Actor::Operator, "Dividend");
    p.legs = draw(&lots, &[(Dest::External { party: investor }, amount)])?;
    Ok(p)
}

pub fn expense(s: &State, vendor: PartyId, amount: Money, purpose: &str) -> R<TxProposal> {
    let lots = s.class_lots(MoneyClass::MudaribShare);
    let mut p = TxProposal::new(TxKind::Expense, Actor::Operator, purpose);
    p.legs = draw(&lots, &[(Dest::External { party: vendor }, amount)])?;
    Ok(p)
}

pub fn donation(s: &State, charity: PartyId, amount: Money, purpose: &str) -> R<TxProposal> {
    let lots = s.class_lots(MoneyClass::Charity);
    let mut p = TxProposal::new(TxKind::Donation, Actor::Operator, purpose);
    p.legs = draw(&lots, &[(Dest::External { party: charity }, amount)])?;
    Ok(p)
}

/// Defolt: qolgan asosiy qism pozitsiya egalariga pro-rata hisobdan chiqariladi.
/// Mudarib aybi bo'lsa — MudaribShare'dan imkon qadar qoplanadi.
pub fn loss_allocation(s: &State, contract_id: ContractId, fault: Fault) -> R<TxProposal> {
    let c = contract(s, contract_id)?;
    let weights: Vec<(PartyId, Money)> = s
        .contract_positions(contract_id)
        .iter()
        .map(|p| (p.investor, p.outstanding()))
        .filter(|w| w.1.is_positive())
        .collect();
    let mut p = TxProposal::new(TxKind::LossAllocation, Actor::Operator, format!("Defolt: {}", c.goods));
    p.contract_id = Some(contract_id);
    p.fault = Some(fault);
    p.write_offs = weights.iter().map(|(investor, amount)| Share { investor: *investor, amount: *amount }).collect();
    if fault == Fault::Mudarib {
        let loss: Money = weights.iter().map(|w| w.1).sum();
        let lots = s.class_lots(MoneyClass::MudaribShare);
        let cover = loss.min(State::sum(&lots));
        let targets: Vec<(Dest, Money)> = split_pro_rata(cover, &weights)
            .into_iter()
            .filter(|t| t.1.is_positive())
            .map(|(o, a)| (capital(o), a))
            .collect();
        p.legs = draw(&lots, &targets)?;
    }
    Ok(p)
}

/// Rezervdan defolt zararini qoplash (Sharia board qarori bilan bajariladi).
pub fn reserve_cover(s: &State, contract_id: ContractId, amount: Option<Money>) -> R<TxProposal> {
    let c = contract(s, contract_id)?;
    let weights: Vec<(PartyId, Money)> = s
        .contract_positions(contract_id)
        .iter()
        .map(|p| (p.investor, p.uncovered_loss()))
        .filter(|w| w.1.is_positive())
        .collect();
    let uncovered: Money = weights.iter().map(|w| w.1).sum();
    let lots = s.class_lots(MoneyClass::Reserve);
    let total = amount.unwrap_or(uncovered).min(uncovered).min(State::sum(&lots));
    if !total.is_positive() {
        return Err(BuildError::Invalid("qoplanadigan zarar yoki rezerv yo'q".into()));
    }
    let targets: Vec<(Dest, Money)> = split_pro_rata(total, &weights)
        .into_iter()
        .filter(|t| t.1.is_positive())
        .map(|(o, a)| (capital(o), a))
        .collect();
    let mut p = TxProposal::new(TxKind::ReserveUse, Actor::Operator, format!("Rezervdan zarar qoplash: {}", c.goods));
    p.contract_id = Some(contract_id);
    p.legs = draw(&lots, &targets)?;
    Ok(p)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub date: NaiveDate,
    pub amount: Money,
    pub cumulative: Money,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WithdrawalPlan {
    pub requested: Money,
    pub payable_now: Money,
    pub remaining: Money,
    /// Qolgan qism qachon bo'shashi (to'lov jadvali bo'yicha taxminiy).
    pub schedule: Vec<Release>,
    pub proposal: Option<TxProposal>,
}

/// T13: likvid qism darhol, qolgani faqat to'lov jadvali bo'yicha — boshqa investor pulidan emas.
pub fn withdrawal(s: &State, investor: PartyId, requested: Money, by: Actor, today: NaiveDate) -> R<WithdrawalPlan> {
    if !s.is_party(investor, PartyKind::Investor) {
        return Err(BuildError::NotFound(format!("investor {investor}")));
    }
    if !requested.is_positive() {
        return Err(BuildError::Invalid("summa musbat bo'lishi kerak".into()));
    }
    let lots = s.owner_lots(investor, MoneyClass::Capital);
    let liquid = State::sum(&lots);
    let payable_now = requested.min(liquid);
    let remaining = requested - payable_now;
    let proposal = if payable_now.is_positive() {
        let mut p = TxProposal::new(TxKind::Withdrawal, by, "Kapitalni yechib olish");
        p.legs = draw(&lots, &[(Dest::External { party: investor }, payable_now)])?;
        Some(p)
    } else {
        None
    };
    let schedule = release_schedule(s, investor, today, remaining);
    Ok(WithdrawalPlan { requested, payable_now, remaining, schedule, proposal })
}

/// Investor kapitalining faol shartnomalardan qaytish jadvali (taxminiy, kafolatlanmagan).
pub fn release_schedule(s: &State, investor: PartyId, today: NaiveDate, up_to: Money) -> Vec<Release> {
    let mut by_date: BTreeMap<NaiveDate, Money> = BTreeMap::new();
    for pos in s.investor_positions(investor) {
        let Some(c) = s.contract(pos.contract_id) else { continue };
        if !matches!(c.effective_status(today), ContractStatus::Active | ContractStatus::Overdue) {
            continue;
        }
        let remaining = c.remaining_principal();
        if !remaining.is_positive() || !pos.outstanding().is_positive() {
            continue;
        }
        let mut cum = Money::ZERO;
        for inst in &c.schedule {
            let before = cum;
            cum += inst.principal;
            if cum <= c.paid_principal {
                continue;
            }
            let unpaid = cum - before.max(c.paid_principal);
            let part = unpaid.mul_div_floor(pos.outstanding(), remaining);
            if part.is_positive() {
                *by_date.entry(inst.due.max(today)).or_insert(Money::ZERO) += part;
            }
        }
    }
    let mut out = Vec::new();
    let mut cumulative = Money::ZERO;
    for (date, amount) in by_date {
        if cumulative >= up_to {
            break;
        }
        cumulative += amount;
        out.push(Release { date, amount, cumulative });
    }
    out
}
