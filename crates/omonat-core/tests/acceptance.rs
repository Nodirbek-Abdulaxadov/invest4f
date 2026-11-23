//! Qabul mezonlari (plan.md §17): har bir invariant test bilan isbotlanadi.
//! T12 (bank Omonat imzosisiz to'lamaydi) — `omonat-bank` crate'ida.

use chrono::{DateTime, Duration, TimeZone, Utc};
use omonat_core::builders;
use omonat_core::chain::{self, entry_hash, verify_chain, LedgerEntry, GENESIS_HASH};
use omonat_core::engine::{self, CmdResult, Decision, Engine, NewContract, Outcome};
use omonat_core::event::Event;
use omonat_core::local_date;
use omonat_core::model::*;
use omonat_core::money::Money;
use omonat_core::policy::config::Policy;
use omonat_core::trace;
use omonat_core::views;
use uuid::Uuid;

use MoneyClass::*;

fn som(n: i64) -> Money {
    Money::som(n)
}

fn sharia(subject: Uuid, at: DateTime<Utc>) -> Approval {
    Approval {
        subject,
        role: Role::ShariaBoard,
        signer: "Sharia board raisi".into(),
        signature: "00".into(),
        public_key: "00".into(),
        at,
    }
}

fn rejected_by(o: &Outcome, rule: &str) -> bool {
    matches!(o, Outcome::Rejected { violations } if violations.iter().any(|v| v.rule_id == rule))
}

fn executed(o: &Outcome) -> Vec<PaymentId> {
    match o {
        Outcome::Executed { payments, .. } => payments.clone(),
        other => panic!("bajarilishi kutilgan edi: {other:?}"),
    }
}

struct World {
    e: Engine,
    entries: Vec<LedgerEntry>,
    now: DateTime<Utc>,
    aziz: PartyId,
    bobur: PartyId,
    nodira: PartyId,
    merchant: PartyId,
    customer: PartyId,
    marketing: PartyId,
    related: PartyId,
    charity: PartyId,
    bank_seq: u32,
}

impl World {
    fn new() -> World {
        let mut w = World {
            e: Engine::new(),
            entries: Vec::new(),
            now: Utc.with_ymd_and_hms(2026, 11, 2, 6, 0, 0).unwrap(),
            aziz: Uuid::new_v4(),
            bobur: Uuid::new_v4(),
            nodira: Uuid::new_v4(),
            merchant: Uuid::new_v4(),
            customer: Uuid::new_v4(),
            marketing: Uuid::new_v4(),
            related: Uuid::new_v4(),
            charity: Uuid::new_v4(),
            bank_seq: 0,
        };
        let parties = [
            (w.aziz, PartyKind::Investor, "Aziz"),
            (w.bobur, PartyKind::Investor, "Bobur"),
            (w.nodira, PartyKind::Investor, "Nodira"),
            (w.merchant, PartyKind::Merchant, "Texnomart"),
            (w.customer, PartyKind::Customer, "Mijoz Karimov"),
            (w.marketing, PartyKind::Vendor, "Reklama agentligi"),
            (w.related, PartyKind::Vendor, "Rahbar qarindoshi MChJ"),
            (w.charity, PartyKind::Charity, "Mehr xayriya jamg'armasi"),
        ];
        for (i, (id, kind, name)) in parties.into_iter().enumerate() {
            let party = Party { id, kind, name: name.into(), bank_account: format!("20208000{i:012}") };
            w.ok(w.e.register_party(party, Actor::Operator));
        }
        let policy = Policy {
            version: 1,
            reserve_cap_bps: 500,
            mudarib_share_bps: 3000,
            related_parties: vec![w.related],
            approved_charities: vec![w.charity],
            product_description: "Mudaraba investitsiya hisobi. Foyda kafolatlanmagan.".into(),
        };
        let r = w.e.propose_policy(policy, Actor::Operator, w.now);
        w.ok(r);
        let r = w.e.activate_policy(1, sharia(Uuid::nil(), w.now));
        w.ok(r);
        w
    }

    fn commit(&mut self, d: Decision) -> Outcome {
        let entries = self.e.commit(&d.events, self.now);
        self.entries.extend(entries);
        d.outcome
    }

    fn ok(&mut self, r: CmdResult) -> Outcome {
        self.commit(r.expect("buyruq qabul qilinishi kerak"))
    }

    fn propose(&mut self, p: TxProposal) -> Outcome {
        let r = self.e.propose_tx(p, self.now);
        self.ok(r)
    }

    fn next_ref(&mut self) -> String {
        self.bank_seq += 1;
        format!("BANK-{}", self.bank_seq)
    }

    fn deposit(&mut self, investor: PartyId, amount: Money) -> Outcome {
        let r = self.next_ref();
        self.propose(builders::deposit(investor, amount, &r))
    }

    fn confirm_all(&mut self) {
        let s = self.e.state();
        let pending: Vec<PaymentId> =
            s.payments.keys().filter(|id| s.payment_status(**id) == PaymentStatus::Pending).cloned().collect();
        for id in pending {
            let r = self.e.payment_confirmed(id, format!("BR-{id}"));
            self.ok(r);
        }
    }

    fn contract(&mut self, cost: Money, markup: Money, n: u32) -> ContractId {
        let input = NewContract {
            merchant: self.merchant,
            customer: self.customer,
            goods: "Muzlatgich".into(),
            cost,
            markup,
            installments: n,
            first_due: local_date(self.now) + Duration::days(30),
        };
        let d = self.e.create_contract(input, Actor::Operator, self.now).unwrap();
        let Event::ContractCreated { contract, .. } = &d.events[0] else { panic!() };
        let id = contract.id;
        self.commit(d);
        id
    }

    fn attach_doc(&mut self, cid: ContractId) {
        let doc = OwnershipDoc {
            doc_no: "QABZ-001".into(),
            description: "Yuk xati va qabz".into(),
            sha256: "ab".repeat(32),
            attached_at: self.now,
        };
        let r = self.e.attach_ownership_doc(cid, doc, Actor::Operator);
        self.ok(r);
    }

    fn active_contract(&mut self, cost: Money, markup: Money, n: u32) -> ContractId {
        let cid = self.contract(cost, markup, n);
        let p = builders::deploy(self.e.state(), cid).unwrap();
        executed(&self.propose(p));
        self.confirm_all();
        self.attach_doc(cid);
        let r = self.e.activate_contract(cid, Actor::Operator, self.now);
        assert_eq!(self.ok(r), Outcome::Recorded);
        cid
    }

    fn installment(&mut self, cid: ContractId, amount: Money, late_fee: Money) -> Outcome {
        let r = self.next_ref();
        let p = builders::installment(self.e.state(), cid, amount, late_fee, &r).unwrap();
        self.propose(p)
    }

    fn first_lot(&self, owner: PartyId, class: MoneyClass) -> LotId {
        self.e.state().owner_lots(owner, class)[0].id
    }
}

// ---------------------------------------------------------------------------------------------

#[test]
fn t1_dividend_from_new_investor_capital_is_rejected() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    w.deposit(w.nodira, som(5_000_000));
    let lot = w.first_lot(w.nodira, Capital);
    let mut p = TxProposal::new(TxKind::Dividend, Actor::Operator, "Oylik dividend");
    p.legs.push(Leg { source: Source::Lot { lot_id: lot }, dest: Dest::External { party: w.aziz }, amount: som(500_000) });

    let o = w.propose(p);
    assert!(rejected_by(&o, "I1"), "{o:?}");
    assert!(w.e.state().payments.is_empty());
    assert!(w.e.state().is_active(lot));
    assert_eq!(w.e.state().rejected.len(), 1, "rad etilgan urinish log'ga yoziladi");
}

#[test]
fn t2_withdrawal_from_other_investors_lots_is_rejected() {
    let mut w = World::new();
    w.deposit(w.aziz, som(1_000_000));
    w.deposit(w.bobur, som(3_000_000));
    let bobur_lot = w.first_lot(w.bobur, Capital);
    let mut p = TxProposal::new(TxKind::Withdrawal, Actor::Operator, "Kapitalni yechish");
    p.legs.push(Leg { source: Source::Lot { lot_id: bobur_lot }, dest: Dest::External { party: w.aziz }, amount: som(2_000_000) });

    let o = w.propose(p);
    assert!(rejected_by(&o, "I2"), "{o:?}");
    assert_eq!(w.e.state().owner_balance(w.bobur, Capital), som(3_000_000));
}

#[test]
fn t3_marketing_payment_from_capital_is_rejected() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    let lot = w.first_lot(w.aziz, Capital);
    let mut p = TxProposal::new(TxKind::Expense, Actor::Operator, "Reklama kampaniyasi");
    p.legs.push(Leg { source: Source::Lot { lot_id: lot }, dest: Dest::External { party: w.marketing }, amount: som(800_000) });

    let o = w.propose(p);
    assert!(rejected_by(&o, "I3"), "{o:?}");
}

#[test]
fn t4_activation_without_goods_purchase_is_rejected() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    let cid = w.contract(som(1_000_000), som(200_000), 4);

    let r = w.e.activate_contract(cid, Actor::Operator, w.now);
    assert!(rejected_by(&w.ok(r), "I4"));

    // Xarid taklif qilingan va hujjat bor, lekin bank to'lovni hali tasdiqlamagan.
    let p = builders::deploy(w.e.state(), cid).unwrap();
    executed(&w.propose(p));
    w.attach_doc(cid);
    let r = w.e.activate_contract(cid, Actor::Operator, w.now);
    assert!(rejected_by(&w.ok(r), "I4"));

    // Mijoz to'lovi faol bo'lmagan shartnoma bo'yicha qabul qilinmaydi.
    let mut inst = TxProposal::new(TxKind::Installment, Actor::Bank, "Mijoz to'lovi");
    inst.contract_id = Some(cid);
    inst.bank_ref = Some("BANK-X".into());
    inst.legs.push(Leg { source: Source::Inflow, dest: Dest::Lot { class: Profit, owner: None }, amount: som(10) });
    assert!(rejected_by(&w.propose(inst), "I4"));

    w.confirm_all();
    let r = w.e.activate_contract(cid, Actor::Operator, w.now);
    assert_eq!(w.ok(r), Outcome::Recorded);
    assert_eq!(w.e.state().contract(cid).unwrap().status, ContractStatus::Active);
}

#[test]
fn t5_reserve_above_cap_by_one_som_is_rejected() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    let cid = w.active_contract(som(1_000_000), som(200_000), 4);
    executed(&w.installment(cid, som(300_000), Money::ZERO));

    let s = w.e.state();
    let lots = s.active_lots(|l| l.class == Profit && l.contract_id == Some(cid));
    let profit: Money = lots.iter().map(|l| l.amount).sum();
    let cap = profit.mul_bps_floor(500);
    let reserve = cap + som(1);
    let mudarib = (profit - reserve).mul_bps_floor(3000);
    let investors = profit - reserve - mudarib;
    let targets = vec![
        (Dest::Lot { class: Reserve, owner: None }, reserve),
        (Dest::Lot { class: MudaribShare, owner: None }, mudarib),
        (Dest::Lot { class: InvestorProfit, owner: Some(w.aziz) }, investors),
    ];
    let mut p = TxProposal::new(TxKind::ProfitSplit, Actor::Operator, "Foyda taqsimoti");
    p.contract_id = Some(cid);
    p.legs = builders::draw(&lots, &targets).unwrap();
    assert!(rejected_by(&w.propose(p), "I5"));

    // Aynan cap — ruxsat.
    let p = builders::profit_split(w.e.state(), cid, 500).unwrap();
    executed(&w.propose(p));
    assert_eq!(w.e.state().class_balance(Reserve), cap);
}

#[test]
fn t6_late_fee_into_profit_is_rejected() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    let cid = w.active_contract(som(1_000_000), som(200_000), 4);

    let mut p = builders::installment(w.e.state(), cid, som(350_000), som(50_000), "BANK-77").unwrap();
    p.legs.retain(|l| !matches!(l.dest, Dest::Lot { class: Charity, .. }));
    for l in &mut p.legs {
        if matches!(l.dest, Dest::Lot { class: Profit, .. }) {
            l.amount += som(50_000);
        }
    }
    assert!(rejected_by(&w.propose(p), "I7"));

    executed(&w.installment(cid, som(350_000), som(50_000)));
    assert_eq!(w.e.state().class_balance(Charity), som(50_000));
    assert_eq!(w.e.state().class_balance(Profit), som(50_000));
}

#[test]
fn t7_related_party_payment_waits_for_sharia_board() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    let cid = w.active_contract(som(1_000_000), som(200_000), 4);
    executed(&w.installment(cid, som(300_000), Money::ZERO));
    let p = builders::profit_split(w.e.state(), cid, 500).unwrap();
    executed(&w.propose(p));
    assert!(w.e.state().class_balance(MudaribShare).is_positive());

    let p = builders::expense(w.e.state(), w.related, som(10_000), "Ofis ijarasi").unwrap();
    let pid = p.id;
    let payments_before = w.e.state().payments.len();
    let o = w.propose(p);
    assert!(matches!(&o, Outcome::PendingApproval { needs, .. } if needs.iter().any(|n| n.rule_id == "I8")), "{o:?}");
    assert_eq!(w.e.state().payments.len(), payments_before, "imzosiz to'lov bankka chiqmaydi");

    // Operator o'zi ikkinchi imzo qo'ya olmaydi.
    let mut fake = sharia(pid, w.now);
    fake.role = Role::Operator;
    assert!(w.e.approve(pid, fake, w.now).is_err());

    let r = w.e.approve(pid, sharia(pid, w.now), w.now);
    let pays = executed(&w.ok(r));
    assert_eq!(pays.len(), 1);
    assert_eq!(w.e.state().payments[&pays[0]].beneficiary, w.related);
}

#[test]
fn t8_default_loss_is_pro_rata_and_sums_exactly() {
    let mut w = World::new();
    w.deposit(w.aziz, Money(300_000_007));
    w.deposit(w.bobur, Money(500_000_003));
    w.deposit(w.nodira, Money(200_000_011));
    let cid = w.active_contract(Money(700_000_001), Money(140_000_000), 6);
    let first = w.e.state().contract(cid).unwrap().schedule[0].clone();
    executed(&w.installment(cid, first.principal + first.markup, Money::ZERO));

    w.now += Duration::days(70);
    let before: Vec<(PartyId, Money)> =
        w.e.state().contract_positions(cid).iter().map(|p| (p.investor, p.outstanding())).collect();
    assert_eq!(before.len(), 3);
    let remaining = w.e.state().contract(cid).unwrap().remaining_principal();

    let good = builders::loss_allocation(w.e.state(), cid, Fault::None).unwrap();
    let mut bad = good.clone();
    bad.id = Uuid::new_v4();
    bad.write_offs[0].amount -= Money(1);
    bad.write_offs[1].amount += Money(1);
    assert!(rejected_by(&w.propose(bad), "I10"));

    executed(&w.propose(good));
    let s = w.e.state();
    let written: Money = s.contract_positions(cid).iter().map(|p| p.written_off).sum();
    assert_eq!(written, remaining, "zarar yig'indisi qolgan asosiy qismga aniq teng");
    for (inv, out) in before {
        let p = s.position(cid, inv).unwrap();
        assert_eq!(p.written_off, out);
        assert_eq!(p.outstanding(), Money::ZERO);
    }
    assert_eq!(s.contract(cid).unwrap().status, ContractStatus::Defaulted);
}

fn rehash(entries: &[LedgerEntry]) -> Vec<LedgerEntry> {
    let mut prev = GENESIS_HASH;
    entries
        .iter()
        .map(|e| {
            let mut e = e.clone();
            e.prev_hash = hex::encode(prev);
            let h = entry_hash(&prev, e.seq, e.at, &e.kind, &e.payload);
            e.hash = hex::encode(h);
            prev = h;
            e
        })
        .collect()
}

#[test]
fn t9_manual_update_breaks_hash_chain() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    w.deposit(w.bobur, som(2_000_000));
    assert!(verify_chain(&w.entries).is_ok());

    let day = local_date(w.now);
    let entries = w.entries.clone();
    let r = w.e.anchor(day, &entries, |_| ("sig".into(), "pk".into()), w.now);
    w.ok(r);

    let idx = w.entries.iter().position(|e| e.kind == "TxApplied" && e.payload.contains("1000000000")).unwrap();
    let mut tampered = w.entries.clone();
    tampered[idx].payload = tampered[idx].payload.replacen("1000000000", "9000000000", 1);
    let err = verify_chain(&tampered).unwrap_err();
    assert_eq!(err.seq, tampered[idx].seq);
    assert!(Engine::replay(&tampered).is_err());

    // Hujumchi keyingi barcha xeshlarni qayta hisoblasa ham — e'lon qilingan Merkle root mos kelmaydi.
    let rewritten = rehash(&tampered);
    assert!(verify_chain(&rewritten).is_ok());
    let anchors = w.e.state().anchors.clone();
    assert!(engine::verify_anchors(&rewritten, &anchors).iter().any(|c| !c.ok));
    assert!(engine::verify_anchors(&w.entries, &anchors).iter().all(|c| c.ok));
}

#[test]
fn t10_ledger_bank_mismatch_opens_incident() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    let expected = w.e.state().expected_bank_balance();
    assert_eq!(expected, som(10_000_000));

    let d = w.e.reconcile(expected, w.now);
    w.commit(d);
    assert!(w.e.state().incidents.is_empty());

    let d = w.e.reconcile(expected - som(1), w.now);
    w.commit(d);
    let s = w.e.state();
    assert!(!s.reconciliations.last().unwrap().ok);
    assert_eq!(s.incidents.len(), 1);
    assert_eq!(s.incidents[0].kind, IncidentKind::ReconciliationMismatch);
}

#[test]
fn t11_dividend_lineage_reaches_contract_markup() {
    let mut w = World::new();
    w.deposit(w.aziz, som(6_000_000));
    w.deposit(w.bobur, som(4_000_000));
    let cid = w.active_contract(som(1_000_000), som(200_000), 4);
    executed(&w.installment(cid, som(300_000), Money::ZERO));
    let p = builders::profit_split(w.e.state(), cid, 500).unwrap();
    executed(&w.propose(p));
    let p = builders::dividend(w.e.state(), w.aziz, None).unwrap();
    let pays = executed(&w.propose(p));

    let s = w.e.state();
    let lin = trace::payment_lineage(s, pays[0]).unwrap();
    assert!(!lin.branches.is_empty());
    for b in &lin.branches {
        let first = b.lineage.first().unwrap();
        assert_eq!((first.class, first.owner), (InvestorProfit, Some(w.aziz)));
        let root = b.lineage.last().unwrap();
        assert_eq!(root.class, Profit);
        assert_eq!(root.tx_kind, Some(TxKind::Installment));
        assert_eq!(root.contract_id, Some(cid));
        assert!(root.bank_ref.is_some());
    }
    let dash = views::investor_dashboard(s, w.aziz, local_date(w.now)).unwrap();
    let src = &dash.dividends[0].sources[0];
    assert_eq!(src.contract_id, Some(cid));
    assert_eq!(src.month.as_deref(), Some("2026-11"));
}

#[test]
fn t13_withdrawal_beyond_liquidity_pays_liquid_part_and_shows_schedule() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    w.active_contract(som(8_800_000), som(1_000_000), 6);
    w.deposit(w.bobur, som(5_000_000));

    let today = local_date(w.now);
    let plan = builders::withdrawal(w.e.state(), w.aziz, som(3_000_000), Actor::Investor(w.aziz), today).unwrap();
    assert_eq!(plan.payable_now, som(1_200_000));
    assert_eq!(plan.remaining, som(1_800_000));
    assert!(!plan.schedule.is_empty());
    assert!(plan.schedule.last().unwrap().cumulative >= plan.remaining);
    assert!(plan.schedule.windows(2).all(|x| x[0].date < x[1].date));

    let pays = executed(&w.propose(plan.proposal.unwrap()));
    let s = w.e.state();
    assert_eq!(s.payments[&pays[0]].amount, som(1_200_000));
    assert_eq!(s.owner_balance(w.aziz, Capital), Money::ZERO);
    assert_eq!(s.owner_balance(w.bobur, Capital), som(5_000_000), "boshqa investor puliga tegilmaydi");
}

// --------------------------------------------------------------------------------- qo'shimcha

#[test]
fn i13_guaranteed_return_in_config_is_rejected() {
    let bad = r#"{"version":2,"reserve_cap_bps":500,"mudarib_share_bps":3000,"guaranteed_return_bps":2000}"#;
    let err = Policy::from_json(bad).unwrap_err();
    assert!(err.iter().any(|v| v.rule_id == "I13"));

    let promise = r#"{"version":2,"reserve_cap_bps":500,"mudarib_share_bps":3000,"product_description":"Yiliga 24% kafolatlangan daromad!"}"#;
    assert!(Policy::from_json(promise).unwrap_err().iter().any(|v| v.rule_id == "I13"));

    let fine = r#"{"version":2,"reserve_cap_bps":500,"mudarib_share_bps":3000,"product_description":"Foyda kafolatlanmagan."}"#;
    assert!(Policy::from_json(fine).is_ok());
}

#[test]
fn bank_rejection_writes_compensation_and_restores_lots() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    let cid = w.contract(som(1_000_000), som(200_000), 4);
    let p = builders::deploy(w.e.state(), cid).unwrap();
    let pays = executed(&w.propose(p));
    assert_eq!(w.e.state().owner_balance(w.aziz, Capital), som(9_000_000));

    let r = w.e.payment_failed(pays[0], "Merchant hisobi yopilgan".into(), w.now);
    executed(&w.ok(r));
    let s = w.e.state();
    assert_eq!(s.owner_balance(w.aziz, Capital), som(10_000_000));
    assert_eq!(s.expected_bank_balance(), som(10_000_000));
    assert!(s.contract_positions(cid).is_empty());
    assert_eq!(s.contract(cid).unwrap().status, ContractStatus::Draft);
    assert!(s.contract(cid).unwrap().purchase_tx.is_none());
}

#[test]
fn frozen_lot_cannot_be_spent_until_released() {
    let mut w = World::new();
    w.deposit(w.nodira, som(2_000_000));
    let lot = w.first_lot(w.nodira, Capital);
    let r = w.e.freeze(lot, 5, 90, "O'g'irlangan karta shubhasi".into(), Actor::Admin, w.now);
    w.ok(r);
    assert_eq!(w.e.state().frozen_balance(Some(w.nodira)), som(2_000_000));

    let today = local_date(w.now);
    let plan = builders::withdrawal(w.e.state(), w.nodira, som(1_000_000), Actor::Operator, today).unwrap();
    assert_eq!(plan.payable_now, Money::ZERO, "muzlatilgan pul likvid emas");

    let mut p = TxProposal::new(TxKind::Withdrawal, Actor::Operator, "Yechish");
    p.legs.push(Leg { source: Source::Lot { lot_id: lot }, dest: Dest::External { party: w.nodira }, amount: som(1_000_000) });
    assert!(rejected_by(&w.propose(p.clone()), "STRUCT"));

    let case = *w.e.state().freezes.keys().next().unwrap();
    let r = w.e.unfreeze(case, Actor::Admin, "Tekshiruv yakunlandi".into());
    w.ok(r);
    p.id = Uuid::new_v4();
    executed(&w.propose(p));
}

#[test]
fn replay_rebuilds_identical_state() {
    let mut w = World::new();
    w.deposit(w.aziz, som(6_000_000));
    w.deposit(w.bobur, som(4_000_000));
    let cid = w.active_contract(som(1_000_000), som(200_000), 4);
    executed(&w.installment(cid, som(300_000), som(5_000)));
    let p = builders::profit_split(w.e.state(), cid, 300).unwrap();
    executed(&w.propose(p));

    let replayed = Engine::replay(&w.entries).unwrap();
    let (a, b) = (w.e.state(), replayed.state());
    assert_eq!(a.head_seq, b.head_seq);
    assert_eq!(a.head_hash, b.head_hash);
    assert_eq!(a.expected_bank_balance(), b.expected_bank_balance());
    assert_eq!(views::class_balances(a).len(), views::class_balances(b).len());
    for (x, y) in views::class_balances(a).iter().zip(views::class_balances(b).iter()) {
        assert_eq!(x.amount, y.amount);
    }
    assert_eq!(a.positions, b.positions);
    assert_eq!(chain::merkle_root_of(&w.entries, 1, a.head_seq).len(), 64);
}

/// §3.3: Azizning yo'li — kapital, taqsimot, to'lovlar, foyda, defolt, yechib olish.
#[test]
fn aziz_journey() {
    let mut w = World::new();
    w.deposit(w.aziz, som(10_000_000));
    w.deposit(w.bobur, som(10_000_000));
    let contracts: Vec<ContractId> = (0..4).map(|_| w.active_contract(som(4_000_000), som(800_000), 4)).collect();
    let today = local_date(w.now);
    let d = views::investor_dashboard(w.e.state(), w.aziz, today).unwrap();
    assert_eq!(d.liquid, som(2_000_000));
    assert_eq!(d.active, som(8_000_000));
    assert_eq!(d.positions.len(), 4);
    assert!(d.expected_profit.is_positive());
    assert_eq!(d.expected_profit_label, "kafolatlanmagan");

    // 1-oy: uchta shartnoma to'laydi, bittasi to'lamaydi.
    w.now += Duration::days(31);
    for cid in &contracts[..3] {
        executed(&w.installment(*cid, som(1_200_000), Money::ZERO));
        let p = builders::profit_split(w.e.state(), *cid, 500).unwrap();
        executed(&w.propose(p));
    }
    let p = builders::dividend(w.e.state(), w.aziz, None).unwrap();
    executed(&w.propose(p));

    // 3-oy: to'lamagan shartnoma defolt.
    w.now += Duration::days(40);
    let p = builders::loss_allocation(w.e.state(), contracts[3], Fault::None).unwrap();
    executed(&w.propose(p));
    w.confirm_all();

    let today = local_date(w.now);
    let d = views::investor_dashboard(w.e.state(), w.aziz, today).unwrap();
    assert_eq!(d.defaulted_loss, som(2_000_000));
    assert_eq!(d.returned_principal, som(1_500_000));
    assert!(d.dividends_paid.is_positive());
    assert_eq!(d.dividends[0].status, PaymentStatus::Confirmed);

    // 4-oy: 3 mln so'raydi — faqat likvid qism, qolgani jadval bilan.
    let plan = builders::withdrawal(w.e.state(), w.aziz, som(3_000_000), Actor::Investor(w.aziz), today).unwrap();
    assert_eq!(plan.payable_now, som(3_500_000).min(som(3_000_000)));
    let reg = views::regulator_view(w.e.state(), today);
    assert_eq!(reg.written_off, som(4_000_000));
}
