//! Dev: namoyish ma'lumotlari (plan.md §3 ssenariysi asosida). Hammasi oddiy yo'l bilan —
//! bank kirimi, Policy Engine va outbox orqali — yaratiladi.

use crate::app::{App, IncomingKind};
use crate::error::{ApiError, ApiResult};
use chrono::Duration;
use omonat_core::builders;
use omonat_core::engine::NewContract;
use omonat_core::model::*;
use omonat_core::policy::config::Policy;
use omonat_core::{Event, Money, Outcome};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn expect_ok(step: &str, o: Outcome) -> ApiResult<Outcome> {
    match &o {
        Outcome::Rejected { violations } => Err(ApiError::Internal(format!("seed: {step}: {violations:?}"))),
        _ => Ok(o),
    }
}

async fn party(app: &App, kind: PartyKind, name: &str, account: &str) -> ApiResult<PartyId> {
    let p = Party { id: Uuid::new_v4(), kind, name: name.into(), bank_account: account.into() };
    let id = p.id;
    app.exec(move |e, _| Ok(e.register_party(p, Actor::Operator)?)).await?;
    Ok(id)
}

async fn contract(app: &App, merchant: PartyId, customer: PartyId, goods: &str, cost: i64, markup: i64, n: u32, due_in_days: i64) -> ApiResult<ContractId> {
    let first_due = app.today() + Duration::days(due_in_days);
    let input = NewContract {
        merchant,
        customer,
        goods: goods.into(),
        cost: Money::som(cost),
        markup: Money::som(markup),
        installments: n,
        first_due,
    };
    let mut id = None;
    app.exec(|e, now| {
        let d = e.create_contract(input, Actor::Operator, now)?;
        if let Some(Event::ContractCreated { contract, .. }) = d.events.first() {
            id = Some(contract.id);
        }
        Ok(d)
    })
    .await?;
    id.ok_or_else(|| ApiError::Internal("seed: contract id".into()))
}

/// Xarid → bank tasdig'i → egalik hujjati → mijozga sotish.
async fn fund_and_sell(app: &App, cid: ContractId, doc_no: &str) -> ApiResult<()> {
    let o = app.exec(move |e, now| Ok(e.propose_tx(builders::deploy(e.state(), cid)?, now)?)).await?;
    expect_ok("deploy", o)?;
    app.process_outbox().await?;
    let doc = OwnershipDoc {
        doc_no: doc_no.into(),
        description: "Merchant qabzi va yuk xati".into(),
        sha256: hex::encode(Sha256::digest(doc_no.as_bytes())),
        attached_at: app.now(),
    };
    app.exec(move |e, _| Ok(e.attach_ownership_doc(cid, doc, Actor::Operator)?)).await?;
    let o = app.exec(move |e, now| Ok(e.activate_contract(cid, Actor::Operator, now)?)).await?;
    expect_ok("activate", o)?;
    Ok(())
}

pub async fn seed(app: &App) -> ApiResult<Value> {
    if app.read(|e| !e.state().parties.is_empty()).await {
        return Err(ApiError::Conflict("ledger bo'sh emas — seed faqat yangi ledger uchun".into()));
    }
    let aziz = party(app, PartyKind::Investor, "Aziz Karimov", "20206000100000000001").await?;
    let bobur = party(app, PartyKind::Investor, "Bobur Toshmatov", "20206000100000000002").await?;
    let nodira = party(app, PartyKind::Investor, "Nodira Yusupova", "20206000100000000003").await?;
    let dilshod = party(app, PartyKind::Investor, "Dilshod Rahimov", "20206000100000000004").await?;
    let artel = party(app, PartyKind::Merchant, "Artel Savdo MChJ", "20208000200000000001").await?;
    let texno = party(app, PartyKind::Merchant, "Texnomart MChJ", "20208000200000000002").await?;
    let jasur = party(app, PartyKind::Customer, "Jasur Aliyev", "20206000300000000001").await?;
    let malika = party(app, PartyKind::Customer, "Malika Ergasheva", "20206000300000000002").await?;
    let sardor = party(app, PartyKind::Customer, "Sardor Qodirov", "20206000300000000003").await?;
    let reklama = party(app, PartyKind::Vendor, "Reklama Pro MChJ", "20208000400000000001").await?;
    let ofis = party(app, PartyKind::Vendor, "Ofis Servis MChJ", "20208000400000000002").await?;
    let related = party(app, PartyKind::Vendor, "Rahbar qarindoshi MChJ", "20208000400000000003").await?;
    let mehr = party(app, PartyKind::Charity, "Mehr-shafqat jamg'armasi", "20212000500000000001").await?;

    let policy = Policy {
        version: 1,
        reserve_cap_bps: 500,
        mudarib_share_bps: 3000,
        related_parties: vec![related],
        approved_charities: vec![mehr],
        product_description: "Mudaraba investitsiya hisobi: foyda faqat real murabaha savdosidan, kafolatlanmagan; zarar kapitalga mutanosib.".into(),
    };
    let content = serde_json::to_vec(&policy).expect("serializes");
    app.exec(move |e, now| Ok(e.propose_policy(policy, Actor::Operator, now)?)).await?;
    let approval = app.sign_approval(Uuid::from_u128(1), Role::ShariaBoard, &content);
    app.exec(move |e, _| Ok(e.activate_policy(1, approval)?)).await?;

    for (inv, som) in [(aziz, 10_000_000), (bobur, 25_000_000), (nodira, 8_000_000), (dilshod, 15_000_000)] {
        let (o, _) = app.ingest_incoming(IncomingKind::Deposit, inv, Money::som(som), None, Money::ZERO).await?;
        expect_ok("deposit", o)?;
    }

    // C1: faol, bitta to'lov tushgan, foyda taqsimlangan.
    let c1 = contract(app, artel, jasur, "Artel muzlatgich (5 dona, do'kon uchun)", 6_000_000, 1_200_000, 6, 10).await?;
    fund_and_sell(app, c1, "ART-2026-1187").await?;
    let first = app.read(|e| e.state().contract(c1).map(|c| c.schedule[0].clone())).await.expect("c1");
    let (o, _) = app.ingest_incoming(IncomingKind::Installment, jasur, first.principal + first.markup, Some(c1), Money::ZERO).await?;
    expect_ok("installment", o)?;
    let o = app.exec(move |e, now| Ok(e.propose_tx(builders::profit_split(e.state(), c1, 500)?, now)?)).await?;
    expect_ok("profit split", o)?;
    let o = app.exec(move |e, now| Ok(e.propose_tx(builders::dividend(e.state(), aziz, None)?, now)?)).await?;
    expect_ok("dividend", o)?;

    // C2: birinchi to'lov muddati o'tgan — Overdue.
    let c2 = contract(app, texno, malika, "Noutbuklar (o'quv markazi uchun, 8 dona)", 12_000_000, 2_160_000, 12, -45).await?;
    fund_and_sell(app, c2, "TXM-55102").await?;

    // C3: qoralama — operator uchun.
    let c3 = contract(app, artel, sardor, "Konditsionerlar (3 dona)", 4_500_000, 900_000, 6, 30).await?;

    app.process_outbox().await?;
    app.reconcile().await?;

    Ok(json!({
        "investors": { "aziz": aziz, "bobur": bobur, "nodira": nodira, "dilshod": dilshod },
        "merchants": [artel, texno],
        "customers": [jasur, malika, sardor],
        "vendors": { "reklama": reklama, "ofis": ofis, "related": related },
        "charity": mehr,
        "contracts": { "active": c1, "overdue": c2, "draft": c3 },
    }))
}
