//! Invariantlar (plan.md §6). Har bir qoida bitta `PolicyRule`.
//!
//! `STRUCT` — ledger tuzilmasi (lot mavjud va faol, summalar, rollar). `FLOW` — §5'dagi ruxsat
//! etilgan pul oqimlari jadvali; aniq invariantga to'g'ri kelmagan har qanday oqimni ushlaydi.
//! I11 (reconciliation), I12 (hash chain) va I13 (config) tx qoidasi emas — `engine`, `chain` va
//! `policy::config` da.

use super::{PolicyResult, PolicyRule, RuleCtx};
use crate::contract::split_payment;
use crate::local_date;
use crate::model::*;
use crate::money::{split_pro_rata, Money};
use crate::state::State;
use std::collections::BTreeMap;

pub const STRUCT: &str = "STRUCT";
pub const FLOW: &str = "FLOW";

macro_rules! deny {
    ($id:expr, $($arg:tt)*) => {{
        return PolicyResult::Reject { rule_id: $id.to_string(), reason: format!($($arg)*) };
    }};
}

fn src_lot<'a>(leg: &Leg, s: &'a State) -> Option<&'a Lot> {
    match &leg.source {
        Source::Lot { lot_id } => s.lot(*lot_id),
        _ => None,
    }
}

fn src_class(leg: &Leg, s: &State) -> Option<MoneyClass> {
    src_lot(leg, s).map(|l| l.class)
}

fn sum_dest(tx: &TxProposal, class: MoneyClass) -> Money {
    tx.legs
        .iter()
        .filter(|l| matches!(l.dest, Dest::Lot { class: c, .. } if c == class))
        .map(|l| l.amount)
        .sum()
}

fn per_owner_dest(tx: &TxProposal, class: MoneyClass) -> BTreeMap<PartyId, Money> {
    let mut m = BTreeMap::new();
    for l in &tx.legs {
        if let Dest::Lot { class: c, owner: Some(o) } = l.dest {
            if c == class {
                *m.entry(o).or_insert(Money::ZERO) += l.amount;
            }
        }
    }
    m
}

fn nonzero(v: Vec<(PartyId, Money)>) -> BTreeMap<PartyId, Money> {
    v.into_iter().filter(|(_, a)| !a.is_zero()).collect()
}

fn name(s: &State, id: Option<PartyId>) -> String {
    match id.and_then(|i| s.party(i)) {
        Some(p) => p.name.clone(),
        None => id.map(|i| i.to_string()).unwrap_or_else(|| "fond".into()),
    }
}

fn fmt_map(s: &State, m: &BTreeMap<PartyId, Money>) -> String {
    m.iter().map(|(k, v)| format!("{}: {}", name(s, Some(*k)), v)).collect::<Vec<_>>().join("; ")
}

fn contract<'a>(tx: &TxProposal, s: &'a State) -> Option<&'a Contract> {
    tx.contract_id.and_then(|c| s.contract(c))
}

// ---------------------------------------------------------------------------------------------

pub struct Structural;

impl PolicyRule for Structural {
    fn id(&self) -> &'static str {
        STRUCT
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        use TxKind::*;
        let s = ctx.state;
        if tx.legs.is_empty() && tx.kind != LossAllocation {
            deny!(STRUCT, "Tx'da birorta ham leg yo'q");
        }
        if tx.purpose.trim().is_empty() {
            deny!(STRUCT, "To'lov maqsadi (purpose) ko'rsatilmagan");
        }
        let actor_ok = match tx.kind {
            Deposit | Installment => tx.proposed_by == Actor::Bank,
            Compensation => tx.proposed_by == Actor::System,
            Withdrawal => matches!(tx.proposed_by, Actor::Operator | Actor::Investor(_)),
            _ => tx.proposed_by == Actor::Operator,
        };
        if !actor_ok {
            deny!(STRUCT, "{:?} tx'ni {:?} taklif qila olmaydi", tx.kind, tx.proposed_by.role());
        }
        if !matches!(tx.kind, Deposit | Installment | Compensation) && ctx.policy.is_none() {
            deny!(STRUCT, "Faol policy yo'q: Sharia board imzolagan policy versiyasi kerak");
        }
        if matches!(tx.kind, Deploy | Installment | ProfitSplit | LossAllocation | ReserveUse) && contract(tx, s).is_none() {
            deny!(STRUCT, "{:?} uchun mavjud shartnoma ko'rsatilishi kerak", tx.kind);
        }
        if let Some(cid) = tx.contract_id {
            if s.contract(cid).is_none() {
                deny!(STRUCT, "Shartnoma {cid} topilmadi");
            }
        }
        if matches!(tx.kind, Deposit | Installment) && tx.bank_ref.as_deref().map_or(true, |b| b.trim().is_empty()) {
            deny!(STRUCT, "Kiruvchi to'lovda bank havolasi (bank_ref) yo'q");
        }
        if !tx.write_offs.is_empty() && tx.kind != LossAllocation {
            deny!(STRUCT, "write_offs faqat LossAllocation'da");
        }

        let mut drawn: BTreeMap<LotId, Money> = BTreeMap::new();
        for (i, leg) in tx.legs.iter().enumerate() {
            if !leg.amount.is_positive() {
                deny!(STRUCT, "leg {i}: summa musbat bo'lishi kerak");
            }
            match &leg.source {
                Source::Lot { lot_id } => {
                    if s.lot(*lot_id).is_none() {
                        deny!(STRUCT, "leg {i}: lot {lot_id} topilmadi");
                    }
                    match s.lot_status(*lot_id) {
                        Some(LotStatus::Active) => {}
                        Some(LotStatus::Frozen) => deny!(STRUCT, "leg {i}: lot {lot_id} muzlatilgan (freeze)"),
                        _ => deny!(STRUCT, "leg {i}: lot {lot_id} allaqachon sarflangan"),
                    }
                    *drawn.entry(*lot_id).or_insert(Money::ZERO) += leg.amount;
                }
                Source::Inflow => {
                    if !matches!(tx.kind, Deposit | Installment) {
                        deny!(STRUCT, "leg {i}: bankdan kirim faqat Deposit/Installment'da");
                    }
                }
                Source::Refund { .. } => {
                    if tx.kind != Compensation {
                        deny!(STRUCT, "leg {i}: Refund manbasi faqat Compensation'da");
                    }
                }
            }
            match &leg.dest {
                Dest::Lot { class, owner } => {
                    if class.is_owner_bound() {
                        match owner {
                            Some(o) if s.is_party(*o, PartyKind::Investor) => {}
                            _ => deny!(STRUCT, "leg {i}: {class:?} lotining egasi ro'yxatdagi investor bo'lishi kerak"),
                        }
                    } else if owner.is_some() {
                        deny!(STRUCT, "leg {i}: {class:?} fond darajasidagi klass, egasi bo'lmaydi");
                    }
                }
                Dest::External { party } => {
                    if s.party(*party).is_none() {
                        deny!(STRUCT, "leg {i}: oluvchi {party} ro'yxatda yo'q");
                    }
                }
            }
        }
        for (id, d) in drawn {
            let lot = s.lot(id).expect("checked");
            if d > lot.amount {
                deny!(STRUCT, "lot {id}: sarflash {d} lot summasidan {} oshadi", lot.amount);
            }
        }

        if tx.kind == Compensation {
            let Some(pid) = tx.refund_of else { deny!(STRUCT, "Compensation: refund_of ko'rsatilmagan") };
            let Some(pay) = s.payments.get(&pid) else { deny!(STRUCT, "Compensation: to'lov {pid} topilmadi") };
            if s.payment_status(pid) == PaymentStatus::Confirmed {
                deny!(STRUCT, "Compensation: bank bajargan to'lov qaytarilmaydi");
            }
            if s.txs.values().any(|t| t.proposal.kind == Compensation && t.proposal.refund_of == Some(pid)) {
                deny!(STRUCT, "Compensation: to'lov allaqachon qaytarilgan");
            }
            let mut got = Vec::new();
            for leg in &tx.legs {
                match (&leg.source, &leg.dest) {
                    (Source::Refund { lot_id }, Dest::Lot { class, owner }) => {
                        let Some(orig) = s.lot(*lot_id) else { deny!(STRUCT, "Compensation: asl lot topilmadi") };
                        if orig.class != *class || orig.owner != *owner {
                            deny!(STRUCT, "Compensation: pul asl lot egasi va klassiga qaytishi kerak");
                        }
                        got.push((*lot_id, leg.amount));
                    }
                    _ => deny!(STRUCT, "Compensation: faqat Refund → Lot leg'lari"),
                }
            }
            if got != pay.sources {
                deny!(STRUCT, "Compensation: leg'lar asl to'lov manbalariga aynan mos emas");
            }
        }
        PolicyResult::Ok
    }
}

// ---------------------------------------------------------------------------------------------

/// §5: ruxsat etilgan oqimlar. Ro'yxatda yo'q har qanday oqim taqiqlangan.
pub struct Flow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SrcK {
    Inflow,
    Refund,
    Class(MoneyClass),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DestK {
    Lot(MoneyClass),
    External,
}

fn flow_allowed(kind: TxKind, s: SrcK, d: DestK) -> bool {
    use DestK::*;
    use MoneyClass::*;
    use SrcK::*;
    use TxKind::*;
    matches!(
        (kind, s, d),
        (Deposit, Inflow, Lot(Capital))
            | (Installment, Inflow, Lot(Capital | Profit | Charity))
            | (ProfitSplit, Class(Profit), Lot(Reserve | MudaribShare | InvestorProfit))
            | (Deploy, Class(Capital), External)
            | (Dividend, Class(InvestorProfit), External)
            | (Withdrawal, Class(Capital), External)
            | (Expense, Class(MudaribShare), External)
            | (ReserveUse, Class(Reserve), Lot(Capital))
            | (LossAllocation, Class(MudaribShare), Lot(Capital))
            | (Donation, Class(Charity), External)
            | (Compensation, Refund, Lot(_))
    )
}

impl PolicyRule for Flow {
    fn id(&self) -> &'static str {
        FLOW
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        for leg in &tx.legs {
            let sk = match &leg.source {
                Source::Inflow => SrcK::Inflow,
                Source::Refund { .. } => SrcK::Refund,
                Source::Lot { lot_id } => match ctx.state.lot(*lot_id) {
                    Some(l) => SrcK::Class(l.class),
                    None => deny!(FLOW, "lot {lot_id} topilmadi"),
                },
            };
            let dk = match &leg.dest {
                Dest::Lot { class, .. } => DestK::Lot(*class),
                Dest::External { .. } => DestK::External,
            };
            if !flow_allowed(tx.kind, sk, dk) {
                deny!(FLOW, "{:?}: {:?} → {:?} oqimi ruxsat etilmagan", tx.kind, sk, dk);
            }
        }
        PolicyResult::Ok
    }
}

// ---------------------------------------------------------------------------------------------

/// I1: Dividend faqat InvestorProfit lotlaridan, Owner = oluvchi.
pub struct I1Dividend;

impl PolicyRule for I1Dividend {
    fn id(&self) -> &'static str {
        "I1"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let s = ctx.state;
        for leg in &tx.legs {
            let lot = src_lot(leg, s);
            if tx.kind == TxKind::Dividend {
                let Some(l) = lot else { deny!("I1", "Dividend manbasi ledger loti bo'lishi kerak") };
                if l.class != MoneyClass::InvestorProfit {
                    deny!(
                        "I1",
                        "Dividend faqat InvestorProfit lotlaridan to'lanadi; manba {:?} ({}) — realized ustama emas",
                        l.class,
                        name(s, l.owner)
                    );
                }
                match leg.dest {
                    Dest::External { party } if Some(party) == l.owner => {}
                    _ => deny!("I1", "Dividend oluvchisi lot egasi bo'lishi kerak (Owner = oluvchi)"),
                }
            }
            if matches!(leg.dest, Dest::Lot { class: MoneyClass::InvestorProfit, .. })
                && !(tx.kind == TxKind::ProfitSplit && lot.map(|l| l.class) == Some(MoneyClass::Profit))
            {
                deny!("I1", "InvestorProfit faqat realized ustamadan (Profit) ProfitSplit orqali yaratiladi");
            }
        }
        PolicyResult::Ok
    }
}

/// I2: Withdrawal faqat oluvchining Capital lotlaridan; turli egalarning lotlari aralashmaydi.
pub struct I2Segregation;

impl PolicyRule for I2Segregation {
    fn id(&self) -> &'static str {
        "I2"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let s = ctx.state;
        for leg in &tx.legs {
            let lot = src_lot(leg, s);
            if tx.kind == TxKind::Withdrawal {
                let Some(l) = lot.filter(|l| l.class == MoneyClass::Capital) else {
                    deny!("I2", "Withdrawal faqat Capital lotlaridan");
                };
                match leg.dest {
                    Dest::External { party } if Some(party) == l.owner => {}
                    Dest::External { party } => deny!(
                        "I2",
                        "{} ning yechib olishi {} ning lotidan — boshqa investor puli bilan to'lab bo'lmaydi",
                        name(s, Some(party)),
                        name(s, l.owner)
                    ),
                    _ => deny!("I2", "Withdrawal tashqi oluvchiga (investorning o'ziga) bo'ladi"),
                }
                if let Actor::Investor(me) = tx.proposed_by {
                    if l.owner != Some(me) {
                        deny!("I2", "Investor faqat o'z kapitalini yechib oladi");
                    }
                }
            }
            if let Some(l) = lot.filter(|l| l.class.is_owner_bound()) {
                match leg.dest {
                    Dest::Lot { owner: Some(d), .. } if Some(d) != l.owner => deny!(
                        "I2",
                        "Turli egalar lotlari aralashmaydi: {} lotidan {} ga o'tkazib bo'lmaydi",
                        name(s, l.owner),
                        name(s, Some(d))
                    ),
                    Dest::External { party } => {
                        let to_merchant = tx.kind == TxKind::Deploy
                            && contract(tx, s).map(|c| c.merchant == party).unwrap_or(false);
                        if Some(party) != l.owner && !to_merchant {
                            deny!(
                                "I2",
                                "{} ning {:?} lotidan {} ga to'lab bo'lmaydi: faqat egasiga yoki shartnoma merchantiga",
                                name(s, l.owner),
                                l.class,
                                name(s, Some(party))
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        PolicyResult::Ok
    }
}

/// I3: Operatsion xarajat faqat MudaribShare'dan.
pub struct I3Expense;

impl PolicyRule for I3Expense {
    fn id(&self) -> &'static str {
        "I3"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let s = ctx.state;
        for leg in &tx.legs {
            if tx.kind == TxKind::Expense {
                match src_class(leg, s) {
                    Some(MoneyClass::MudaribShare) => {}
                    other => deny!(
                        "I3",
                        "Operatsion xarajat faqat MudaribShare'dan; manba {} — investorlar puli",
                        other.map(|c| format!("{c:?}")).unwrap_or_else(|| "lot emas".into())
                    ),
                }
            }
            if let Dest::External { party } = leg.dest {
                if s.is_party(party, PartyKind::Vendor) && tx.kind != TxKind::Expense {
                    deny!("I3", "Xizmat ko'rsatuvchiga ({}) to'lov faqat Expense sifatida, MudaribShare'dan", name(s, Some(party)));
                }
            }
        }
        PolicyResult::Ok
    }
}

/// I4: Deploy faqat Capital'dan; shartnoma Active bo'lishi uchun xarid va egalik hujjati shart.
pub struct I4Deploy;

/// Shartnomani faollashtirish (mijozga sotish) sharti — ownership-before-sale.
pub fn check_activation(c: &Contract) -> Result<(), Violation> {
    if c.status != ContractStatus::GoodsPurchased {
        return Err(Violation {
            rule_id: "I4".into(),
            reason: format!(
                "Tovar xaridi bankda tasdiqlanmagan (holat {:?}): tovar sotib olinmasdan shartnomani faollashtirib bo'lmaydi",
                c.status
            ),
        });
    }
    if c.ownership_doc.is_none() {
        return Err(Violation {
            rule_id: "I4".into(),
            reason: "Egalik hujjati (qabz) biriktirilmagan: sotishdan oldin egalik isbotlanishi shart".into(),
        });
    }
    Ok(())
}

impl PolicyRule for I4Deploy {
    fn id(&self) -> &'static str {
        "I4"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let s = ctx.state;
        match tx.kind {
            TxKind::Deploy => {
                let Some(c) = contract(tx, s) else { return PolicyResult::Ok };
                if c.status != ContractStatus::Draft || c.purchase_tx.is_some() {
                    deny!("I4", "Shartnoma Draft holatida emas yoki tovar allaqachon xarid qilingan ({:?})", c.status);
                }
                for leg in &tx.legs {
                    if src_class(leg, s) != Some(MoneyClass::Capital) {
                        deny!("I4", "Deploy faqat Capital'dan");
                    }
                    if leg.dest != (Dest::External { party: c.merchant }) {
                        deny!("I4", "Deploy puli faqat shu shartnoma merchantiga ({}) to'lanadi", name(s, Some(c.merchant)));
                    }
                }
                if tx.total() != c.cost {
                    deny!("I4", "Deploy summasi {} tovar tannarxi {} ga teng emas", tx.total(), c.cost);
                }
            }
            TxKind::Installment => {
                let Some(c) = contract(tx, s) else { return PolicyResult::Ok };
                let st = c.effective_status(local_date(ctx.now));
                if !matches!(st, ContractStatus::Active | ContractStatus::Overdue) {
                    deny!("I4", "Shartnoma faol emas ({st:?}): sotilmagan tovar bo'yicha mijoz to'lovi qabul qilinmaydi");
                }
            }
            _ => {}
        }
        PolicyResult::Ok
    }
}

/// I5: Rezervga ajratish ≤ policy cap (% foydadan).
pub struct I5Reserve;

impl PolicyRule for I5Reserve {
    fn id(&self) -> &'static str {
        "I5"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let s = ctx.state;
        for leg in &tx.legs {
            if matches!(leg.dest, Dest::Lot { class: MoneyClass::Reserve, .. })
                && !(tx.kind == TxKind::ProfitSplit && src_class(leg, s) == Some(MoneyClass::Profit))
            {
                deny!("I5", "Rezervga faqat foydadan, ProfitSplit orqali ajratiladi");
            }
        }
        if tx.kind == TxKind::ProfitSplit {
            let Some(pol) = ctx.policy else { return PolicyResult::Ok };
            let profit = tx.total();
            let reserve = sum_dest(tx, MoneyClass::Reserve);
            let cap = profit.mul_bps_floor(pol.reserve_cap_bps);
            if reserve > cap {
                deny!(
                    "I5",
                    "Rezervga {reserve} — policy cap'dan oshadi (foydaning {}% = {cap})",
                    pol.reserve_cap_bps as f64 / 100.0
                );
            }
        }
        PolicyResult::Ok
    }
}

/// I6: MudaribShare = foydaning kelishilgan %, kapitaldan hech qachon.
pub struct I6MudaribShare;

impl PolicyRule for I6MudaribShare {
    fn id(&self) -> &'static str {
        "I6"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let s = ctx.state;
        for leg in &tx.legs {
            if matches!(leg.dest, Dest::Lot { class: MoneyClass::MudaribShare, .. })
                && !(tx.kind == TxKind::ProfitSplit && src_class(leg, s) == Some(MoneyClass::Profit))
            {
                deny!("I6", "MudaribShare faqat foydadan (ProfitSplit) olinadi, kapitaldan hech qachon");
            }
        }
        if tx.kind == TxKind::ProfitSplit {
            let Some(pol) = ctx.policy else { return PolicyResult::Ok };
            let profit = tx.total();
            let reserve = sum_dest(tx, MoneyClass::Reserve);
            let got = sum_dest(tx, MoneyClass::MudaribShare);
            let expected = (profit - reserve).mul_bps_floor(pol.mudarib_share_bps);
            if got != expected {
                deny!(
                    "I6",
                    "Mudarib ulushi {got}; kelishilgan {}% bo'yicha {expected} bo'lishi kerak",
                    pol.mudarib_share_bps as f64 / 100.0
                );
            }
        }
        PolicyResult::Ok
    }
}

/// I7: Kechikish jarimasi faqat Charity'ga; Charity faqat tasdiqlangan oluvchiga.
pub struct I7LateFeeCharity;

impl PolicyRule for I7LateFeeCharity {
    fn id(&self) -> &'static str {
        "I7"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let s = ctx.state;
        let inflow_installment = |leg: &Leg| tx.kind == TxKind::Installment && leg.source == Source::Inflow;
        for leg in &tx.legs {
            if src_class(leg, s) == Some(MoneyClass::Charity) {
                let approved = match leg.dest {
                    Dest::External { party } => ctx.policy.map(|p| p.approved_charities.contains(&party)).unwrap_or(false),
                    _ => false,
                };
                if tx.kind != TxKind::Donation || !approved {
                    deny!("I7", "Charity puli faqat Sharia board tasdiqlagan xayriya tashkilotiga (Donation)");
                }
            }
            match leg.dest {
                Dest::Lot { class: MoneyClass::Charity, .. } if !inflow_installment(leg) => {
                    deny!("I7", "Charity'ga faqat mijozning kechikish jarimasi tushadi");
                }
                Dest::Lot { class: MoneyClass::Profit, .. } if !inflow_installment(leg) => {
                    deny!("I7", "Profit faqat mijozning ustama to'lovidan paydo bo'ladi");
                }
                _ => {}
            }
        }
        if tx.kind == TxKind::Installment {
            let Some(c) = contract(tx, s) else { return PolicyResult::Ok };
            let principal = sum_dest(tx, MoneyClass::Capital);
            let markup = sum_dest(tx, MoneyClass::Profit);
            let applied = principal + markup;
            let remaining = c.price() - c.paid_total();
            if applied > remaining {
                deny!(
                    "I7",
                    "Shartnoma qoldig'idan ({remaining}) ortiq {} asosiy qism/foydaga yozilgan — kechikish jarimasi faqat Charity'ga",
                    applied - remaining
                );
            }
            let (_, expected_markup) = split_payment(c, applied);
            if markup > expected_markup {
                deny!(
                    "I7",
                    "Foydaga {markup} yozilgan, shartnoma ustamasi ulushi {expected_markup}: ortiqcha {} kechikish jarimasi — faqat Charity'ga",
                    markup - expected_markup
                );
            }
        }
        PolicyResult::Ok
    }
}

/// I8: Related-party oluvchiga to'lov — operator + Sharia board (2-of-2).
pub struct I8RelatedParty;

impl PolicyRule for I8RelatedParty {
    fn id(&self) -> &'static str {
        "I8"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let Some(pol) = ctx.policy else { return PolicyResult::Ok };
        for leg in &tx.legs {
            if let Dest::External { party } = leg.dest {
                if pol.related_parties.contains(&party) && !ctx.has_approval(Role::ShariaBoard) {
                    return PolicyResult::NeedsApproval {
                        rule_id: "I8".into(),
                        role: Role::ShariaBoard,
                        reason: format!(
                            "Related party ({}) ga to'lov: operator + Sharia board imzosi (2-of-2) kerak",
                            name(ctx.state, Some(party))
                        ),
                    };
                }
            }
        }
        PolicyResult::Ok
    }
}

/// I9: ReserveUse faqat Sharia board approval bilan.
pub struct I9ReserveUse;

impl PolicyRule for I9ReserveUse {
    fn id(&self) -> &'static str {
        "I9"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let s = ctx.state;
        for leg in &tx.legs {
            if src_class(leg, s) == Some(MoneyClass::Reserve) && tx.kind != TxKind::ReserveUse {
                deny!("I9", "Rezervdan faqat ReserveUse orqali, Sharia board qarori bilan foydalaniladi");
            }
        }
        if tx.kind != TxKind::ReserveUse {
            return PolicyResult::Ok;
        }
        let Some(c) = contract(tx, s) else { return PolicyResult::Ok };
        if c.status != ContractStatus::Defaulted {
            deny!("I9", "Rezerv faqat defolt bo'lgan shartnoma zararini qoplaydi (holat {:?})", c.status);
        }
        for (owner, amount) in per_owner_dest(tx, MoneyClass::Capital) {
            let Some(pos) = s.position(c.id, owner) else {
                deny!("I9", "{} bu shartnomada pozitsiyaga ega emas", name(s, Some(owner)));
            };
            if amount > pos.uncovered_loss() {
                deny!(
                    "I9",
                    "{} uchun qoplash {amount} qoplanmagan zarardan ({}) oshadi",
                    name(s, Some(owner)),
                    pos.uncovered_loss()
                );
            }
        }
        if !ctx.has_approval(Role::ShariaBoard) {
            return PolicyResult::NeedsApproval {
                rule_id: "I9".into(),
                role: Role::ShariaBoard,
                reason: "Rezervdan foydalanish faqat Sharia board qarori bilan".into(),
            };
        }
        PolicyResult::Ok
    }
}

/// I10: To'lov va zarar pozitsiya ulushiga pro-rata; mudarib aybi bo'lsa — mudarib qoplaydi.
pub struct I10ProRata;

impl PolicyRule for I10ProRata {
    fn id(&self) -> &'static str {
        "I10"
    }

    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult {
        let s = ctx.state;
        let Some(c) = contract(tx, s) else { return PolicyResult::Ok };
        let positions = s.contract_positions(c.id);
        match tx.kind {
            TxKind::Installment => {
                let got = per_owner_dest(tx, MoneyClass::Capital);
                let principal: Money = got.values().sum();
                let weights: Vec<(PartyId, Money)> =
                    positions.iter().map(|p| (p.investor, p.outstanding())).filter(|(_, w)| w.is_positive()).collect();
                let outstanding: Money = weights.iter().map(|w| w.1).sum();
                if principal > outstanding {
                    deny!("I10", "Asosiy qism {principal} pozitsiyalardagi qoldiqdan ({outstanding}) oshadi");
                }
                let expected = nonzero(split_pro_rata(principal, &weights));
                if got != expected {
                    deny!(
                        "I10",
                        "Asosiy qism pozitsiya ulushlariga pro-rata taqsimlanmagan. Kutilgan: {}",
                        fmt_map(s, &expected)
                    );
                }
                let markup = sum_dest(tx, MoneyClass::Profit);
                let (_, expected_markup) = split_payment(c, principal + markup);
                if markup < expected_markup {
                    deny!("I10", "Ustama kam yozilgan: {markup}, shartnoma bo'yicha {expected_markup}");
                }
            }
            TxKind::ProfitSplit => {
                for leg in &tx.legs {
                    if src_lot(leg, s).map(|l| l.contract_id) != Some(Some(c.id)) {
                        deny!("I10", "Faqat shu shartnoma foydasi uning pozitsiya egalariga taqsimlanadi");
                    }
                }
                let got = per_owner_dest(tx, MoneyClass::InvestorProfit);
                let total: Money = got.values().sum();
                let weights: Vec<(PartyId, Money)> =
                    positions.iter().map(|p| (p.investor, p.principal)).filter(|(_, w)| w.is_positive()).collect();
                let expected = nonzero(split_pro_rata(total, &weights));
                if got != expected {
                    deny!("I10", "Investor foydasi pozitsiya ulushiga pro-rata emas. Kutilgan: {}", fmt_map(s, &expected));
                }
            }
            TxKind::LossAllocation => {
                let st = c.effective_status(local_date(ctx.now));
                if st != ContractStatus::Overdue {
                    deny!("I10", "Faqat muddati o'tgan (Overdue) shartnoma defolt deb e'lon qilinadi; holat {st:?}");
                }
                let weights: Vec<(PartyId, Money)> =
                    positions.iter().map(|p| (p.investor, p.outstanding())).filter(|(_, w)| w.is_positive()).collect();
                let expected: BTreeMap<PartyId, Money> = weights.iter().cloned().collect();
                let mut got: BTreeMap<PartyId, Money> = BTreeMap::new();
                for w in &tx.write_offs {
                    *got.entry(w.investor).or_insert(Money::ZERO) += w.amount;
                }
                if got != expected {
                    deny!(
                        "I10",
                        "Zarar pozitsiya ulushiga pro-rata taqsimlanmagan. Kutilgan: {}",
                        fmt_map(s, &expected)
                    );
                }
                let cover = per_owner_dest(tx, MoneyClass::Capital);
                match tx.fault.unwrap_or(Fault::None) {
                    Fault::None => {
                        if !cover.is_empty() {
                            deny!("I10", "Mudarib aybi bo'lmasa, zarar pozitsiya egalari zimmasida — qoplash bo'lmaydi");
                        }
                    }
                    Fault::Mudarib => {
                        let loss: Money = weights.iter().map(|w| w.1).sum();
                        let total = loss.min(s.class_balance(MoneyClass::MudaribShare));
                        let expected_cover = nonzero(split_pro_rata(total, &weights));
                        if cover != expected_cover {
                            deny!(
                                "I10",
                                "Mudarib aybi: zarar MudaribShare'dan ({total}) pro-rata qoplanishi kerak. Kutilgan: {}",
                                fmt_map(s, &expected_cover)
                            );
                        }
                    }
                }
            }
            TxKind::ReserveUse => {
                let got = per_owner_dest(tx, MoneyClass::Capital);
                let total: Money = got.values().sum();
                let weights: Vec<(PartyId, Money)> = positions
                    .iter()
                    .map(|p| (p.investor, p.uncovered_loss()))
                    .filter(|(_, w)| w.is_positive())
                    .collect();
                let expected = nonzero(split_pro_rata(total, &weights));
                if got != expected {
                    deny!("I10", "Rezerv qoplovi zarar ulushiga pro-rata emas. Kutilgan: {}", fmt_map(s, &expected));
                }
            }
            _ => {}
        }
        PolicyResult::Ok
    }
}
