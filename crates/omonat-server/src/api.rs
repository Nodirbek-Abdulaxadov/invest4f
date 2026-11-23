//! HTTP API. Har bir yozuvchi so'rov `App::exec` orqali Policy Engine'dan o'tadi.

use crate::app::{App, IncomingKind};
use crate::error::{ApiError, ApiResult, Who};
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use omonat_bank::Signer;
use axum::{Json, Router};
use chrono::{Duration, NaiveDate};
use omonat_core::builders::{self, WithdrawalPlan};
use omonat_core::engine::NewContract;
use omonat_core::model::*;
use omonat_core::policy::config::Policy;
use omonat_core::{trace, views, Money, Outcome};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

type S = State<Arc<App>>;
type J = ApiResult<Json<Value>>;

use Role::{Admin, Investor as InvestorRole, Operator, Regulator, ShariaBoard};

fn ok<T: serde::Serialize>(v: T) -> J {
    Ok(Json(serde_json::to_value(v).map_err(|e| ApiError::Internal(e.to_string()))?))
}

fn outcome(o: Outcome) -> J {
    ok(json!({ "outcome": o }))
}

pub fn router(app: Arc<App>) -> Router {
    let mut r = Router::new()
        .route("/api/health", get(health))
        .route("/api/meta", get(meta))
        .route("/api/parties", get(list_parties).post(create_party))
        .route("/api/policies", get(list_policies).post(propose_policy))
        .route("/api/policies/{version}/approve", post(approve_policy))
        .route("/api/contracts", get(list_contracts).post(create_contract))
        .route("/api/contracts/{id}", get(get_contract))
        .route("/api/contracts/{id}/deploy", post(deploy))
        .route("/api/contracts/{id}/ownership-doc", post(attach_doc))
        .route("/api/contracts/{id}/activate", post(activate))
        .route("/api/contracts/{id}/profit-split", post(profit_split))
        .route("/api/contracts/{id}/default", post(declare_default))
        .route("/api/contracts/{id}/reserve-cover", post(reserve_cover))
        .route("/api/tx", post(propose_raw))
        .route("/api/txs", get(list_txs))
        .route("/api/dividends", post(dividend))
        .route("/api/withdrawals", post(withdraw))
        .route("/api/withdrawals/preview", get(withdraw_preview))
        .route("/api/expenses", post(expense))
        .route("/api/donations", post(donation))
        .route("/api/payments", get(list_payments))
        .route("/api/payments/{id}/lineage", get(payment_lineage))
        .route("/api/approvals", get(list_approvals))
        .route("/api/approvals/{id}/approve", post(approve))
        .route("/api/approvals/{id}/deny", post(deny))
        .route("/api/ledger", get(ledger))
        .route("/api/ledger/verify", get(verify))
        .route("/api/rejected", get(rejected))
        .route("/api/lots", get(list_lots))
        .route("/api/lots/{id}/lineage", get(lot_lineage))
        .route("/api/lots/{id}/trace", get(lot_trace))
        .route("/api/freeze", post(freeze))
        .route("/api/freeze/{id}/release", post(unfreeze))
        .route("/api/reconcile", post(reconcile))
        .route("/api/anchors", get(list_anchors).post(anchor))
        .route("/api/investors/{id}/dashboard", get(investor_dashboard))
        .route("/api/auditor", get(auditor))
        .route("/api/regulator", get(regulator))
        .route("/api/operator", get(operator))
        .route("/api/bank", get(bank));
    if app.dev {
        r = r
            .route("/api/dev/bank/incoming", post(dev_incoming))
            .route("/api/dev/bank/adjust", post(dev_adjust))
            .route("/api/dev/bank/decline-next", post(dev_decline_next))
            .route("/api/dev/tamper", post(dev_tamper))
            .route("/api/dev/clock", post(dev_clock))
            .route("/api/dev/seed", post(dev_seed));
    }
    r.with_state(app)
}

// ------------------------------------------------------------------------------ umumiy

async fn health(State(app): S) -> J {
    let head = app.read(|e| e.state().head_seq).await;
    ok(json!({
        "ok": app.integrity_error().is_none(),
        "integrity_error": app.integrity_error(),
        "store": app.store.kind(),
        "head_seq": head,
        "dev": app.dev,
        "today": app.today(),
        "clock_offset_days": app.clock_offset_days(),
    }))
}

async fn meta(State(app): S, _w: Who) -> J {
    let (parties, policy, active) = app
        .read(|e| {
            let s = e.state();
            (s.parties.values().cloned().collect::<Vec<_>>(), s.policy().cloned(), s.active_policy)
        })
        .await;
    ok(json!({
        "parties": parties,
        "policy": policy,
        "active_policy": active,
        "dev": app.dev,
        "today": app.today(),
        "keys": {
            "omonat": app.keys.omonat.public_key_hex(),
            "operator": app.keys.operator.public_key_hex(),
            "sharia": app.keys.sharia.public_key_hex(),
        },
    }))
}

async fn list_parties(State(app): S, _w: Who) -> J {
    ok(app.read(|e| e.state().parties.values().cloned().collect::<Vec<_>>()).await)
}

#[derive(Deserialize)]
struct NewParty {
    kind: PartyKind,
    name: String,
    bank_account: String,
}

async fn create_party(State(app): S, w: Who, Json(b): Json<NewParty>) -> J {
    w.require(&[Operator, Admin])?;
    let party = Party { id: Uuid::new_v4(), kind: b.kind, name: b.name, bank_account: b.bank_account };
    let id = party.id;
    app.exec(move |e, _| Ok(e.register_party(party, w.0)?)).await?;
    ok(json!({ "id": id }))
}

// ------------------------------------------------------------------------------ policy

async fn list_policies(State(app): S, _w: Who) -> J {
    ok(app.read(|e| e.state().policies.values().cloned().collect::<Vec<_>>()).await)
}

/// Xom JSON qabul qilinadi: I13 — kafolatlangan daromad maydoni bo'lsa rad (log'ga yoziladi).
async fn propose_policy(State(app): S, w: Who, Json(raw): Json<Value>) -> J {
    w.require(&[Operator])?;
    let version = raw.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    match Policy::from_json(&raw.to_string()) {
        Ok(p) => outcome(app.exec(move |e, now| Ok(e.propose_policy(p, w.0, now)?)).await?),
        Err(v) => outcome(app.exec(move |e, now| Ok(e.reject(Attempt::Policy { version, by: w.0 }, v, now))).await?),
    }
}

async fn approve_policy(State(app): S, w: Who, Path(version): Path<u32>) -> J {
    w.require(&[ShariaBoard])?;
    let content = app
        .read(|e| e.state().policies.get(&version).map(|r| serde_json::to_vec(&r.policy).expect("serializes")))
        .await
        .ok_or_else(|| ApiError::NotFound(format!("policy v{version}")))?;
    let approval = app.sign_approval(Uuid::from_u128(version as u128), Role::ShariaBoard, &content);
    outcome(app.exec(move |e, _| Ok(e.activate_policy(version, approval)?)).await?)
}

// ------------------------------------------------------------------------------ shartnomalar

async fn list_contracts(State(app): S, _w: Who) -> J {
    let today = app.today();
    ok(app
        .read(|e| {
            let s = e.state();
            let mut v: Vec<_> = s.contracts.values().map(|c| views::contract_view(s, c, today)).collect();
            v.sort_by_key(|c| std::cmp::Reverse(c.contract.created_at));
            v
        })
        .await)
}

async fn get_contract(State(app): S, _w: Who, Path(id): Path<Uuid>) -> J {
    let today = app.today();
    let v = app
        .read(|e| e.state().contract(id).map(|c| views::contract_view(e.state(), c, today)))
        .await
        .ok_or_else(|| ApiError::NotFound(format!("shartnoma {id}")))?;
    ok(v)
}

async fn create_contract(State(app): S, w: Who, Json(c): Json<NewContract>) -> J {
    w.require(&[Operator])?;
    let mut id = None;
    let o = app
        .exec(|e, now| {
            let d = e.create_contract(c, w.0, now)?;
            if let Some(omonat_core::Event::ContractCreated { contract, .. }) = d.events.first() {
                id = Some(contract.id);
            }
            Ok(d)
        })
        .await?;
    ok(json!({ "outcome": o, "id": id }))
}

async fn deploy(State(app): S, w: Who, Path(id): Path<Uuid>) -> J {
    w.require(&[Operator])?;
    outcome(app.exec(move |e, now| Ok(e.propose_tx(builders::deploy(e.state(), id)?, now)?)).await?)
}

#[derive(Deserialize)]
struct DocBody {
    doc_no: String,
    description: String,
    /// Hujjat matni yoki base64 — xeshi saqlanadi.
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
}

async fn attach_doc(State(app): S, w: Who, Path(id): Path<Uuid>, Json(b): Json<DocBody>) -> J {
    w.require(&[Operator])?;
    let sha256 = match (b.sha256, b.content) {
        (Some(h), _) => h.to_lowercase(),
        (None, Some(c)) => hex::encode(Sha256::digest(c.as_bytes())),
        (None, None) => return Err(ApiError::BadRequest("content yoki sha256 kerak".into())),
    };
    let doc = OwnershipDoc { doc_no: b.doc_no, description: b.description, sha256, attached_at: app.now() };
    outcome(app.exec(move |e, _| Ok(e.attach_ownership_doc(id, doc, w.0)?)).await?)
}

async fn activate(State(app): S, w: Who, Path(id): Path<Uuid>) -> J {
    w.require(&[Operator])?;
    outcome(app.exec(move |e, now| Ok(e.activate_contract(id, w.0, now)?)).await?)
}

#[derive(Deserialize)]
struct SplitBody {
    reserve_bps: u32,
}

async fn profit_split(State(app): S, w: Who, Path(id): Path<Uuid>, Json(b): Json<SplitBody>) -> J {
    w.require(&[Operator])?;
    outcome(app.exec(move |e, now| Ok(e.propose_tx(builders::profit_split(e.state(), id, b.reserve_bps)?, now)?)).await?)
}

#[derive(Deserialize)]
struct DefaultBody {
    fault: Fault,
}

async fn declare_default(State(app): S, w: Who, Path(id): Path<Uuid>, Json(b): Json<DefaultBody>) -> J {
    w.require(&[Operator])?;
    outcome(app.exec(move |e, now| Ok(e.propose_tx(builders::loss_allocation(e.state(), id, b.fault)?, now)?)).await?)
}

#[derive(Deserialize)]
struct AmountOpt {
    #[serde(default)]
    amount: Option<Money>,
}

async fn reserve_cover(State(app): S, w: Who, Path(id): Path<Uuid>, Json(b): Json<AmountOpt>) -> J {
    w.require(&[Operator])?;
    outcome(app.exec(move |e, now| Ok(e.propose_tx(builders::reserve_cover(e.state(), id, b.amount)?, now)?)).await?)
}

// ------------------------------------------------------------------------------ tranzaksiyalar

/// Qo'lda tuzilgan taklif (masalan, auditor namoyishi uchun ataylab noto'g'ri tx).
/// `proposed_by` har doim so'rov egasi bilan almashtiriladi.
async fn propose_raw(State(app): S, w: Who, Json(mut p): Json<TxProposal>) -> J {
    w.require(&[Operator, InvestorRole])?;
    p.proposed_by = w.0;
    if p.id.is_nil() {
        p.id = Uuid::new_v4();
    }
    outcome(app.propose(p).await?)
}

#[derive(Deserialize)]
struct Limit {
    #[serde(default)]
    limit: Option<usize>,
}

async fn list_txs(State(app): S, w: Who, Query(q): Query<Limit>) -> J {
    w.require(&[Operator, ShariaBoard, Regulator, Admin])?;
    ok(app.read(|e| views::recent_txs(e.state(), q.limit.unwrap_or(100))).await)
}

#[derive(Deserialize)]
struct DividendBody {
    investor: PartyId,
    #[serde(default)]
    amount: Option<Money>,
}

async fn dividend(State(app): S, w: Who, Json(b): Json<DividendBody>) -> J {
    w.require(&[Operator])?;
    outcome(app.exec(move |e, now| Ok(e.propose_tx(builders::dividend(e.state(), b.investor, b.amount)?, now)?)).await?)
}

#[derive(Deserialize)]
struct WithdrawBody {
    investor: PartyId,
    amount: Money,
}

fn can_act_for(w: &Who, investor: PartyId) -> ApiResult<()> {
    match w.0 {
        Actor::Operator => Ok(()),
        Actor::Investor(me) if me == investor => Ok(()),
        _ => Err(ApiError::Forbidden("faqat o'z hisobingiz bo'yicha".into())),
    }
}

/// T13: likvid qism darhol to'lanadi, qolgani to'lov jadvali bilan ko'rsatiladi.
async fn withdraw(State(app): S, w: Who, Json(b): Json<WithdrawBody>) -> J {
    can_act_for(&w, b.investor)?;
    let today = app.today();
    let mut plan: Option<WithdrawalPlan> = None;
    let o = app
        .exec(|e, now| {
            let p = builders::withdrawal(e.state(), b.investor, b.amount, w.0, today)?;
            let proposal = p.proposal.clone();
            plan = Some(p);
            match proposal {
                Some(tx) => Ok(e.propose_tx(tx, now)?),
                None => Ok(omonat_core::Decision { events: vec![], outcome: Outcome::Recorded }),
            }
        })
        .await?;
    ok(json!({ "outcome": o, "plan": plan }))
}

#[derive(Deserialize)]
struct PreviewQ {
    investor: PartyId,
    amount: Money,
}

async fn withdraw_preview(State(app): S, w: Who, Query(q): Query<PreviewQ>) -> J {
    can_act_for(&w, q.investor)?;
    let today = app.today();
    let plan = app.read(|e| builders::withdrawal(e.state(), q.investor, q.amount, w.0, today)).await?;
    ok(plan)
}

#[derive(Deserialize)]
struct PayBody {
    party: PartyId,
    amount: Money,
    purpose: String,
}

async fn expense(State(app): S, w: Who, Json(b): Json<PayBody>) -> J {
    w.require(&[Operator])?;
    outcome(app.exec(move |e, now| Ok(e.propose_tx(builders::expense(e.state(), b.party, b.amount, &b.purpose)?, now)?)).await?)
}

async fn donation(State(app): S, w: Who, Json(b): Json<PayBody>) -> J {
    w.require(&[Operator])?;
    outcome(app.exec(move |e, now| Ok(e.propose_tx(builders::donation(e.state(), b.party, b.amount, &b.purpose)?, now)?)).await?)
}

async fn list_payments(State(app): S, w: Who) -> J {
    w.require(&[Operator, ShariaBoard, Regulator, Admin])?;
    ok(app
        .read(|e| {
            let s = e.state();
            let mut v: Vec<_> = s.payments.values().map(|p| views::payment_view(s, p)).collect();
            v.sort_by_key(|p| std::cmp::Reverse(p.payment.created_at));
            v
        })
        .await)
}

async fn payment_lineage(State(app): S, w: Who, Path(id): Path<Uuid>) -> J {
    let l = app
        .read(|e| trace::payment_lineage(e.state(), id))
        .await
        .ok_or_else(|| ApiError::NotFound(format!("to'lov {id}")))?;
    if let Actor::Investor(me) = w.0 {
        if l.payment.beneficiary != me {
            return Err(ApiError::Forbidden("faqat o'z to'lovlaringiz".into()));
        }
    }
    ok(l)
}

// ------------------------------------------------------------------------------ tasdiqlar

async fn list_approvals(State(app): S, w: Who) -> J {
    w.require(&[Operator, ShariaBoard, Regulator, Admin])?;
    ok(app
        .read(|e| {
            let s = e.state();
            let mut history = s.approval_history.clone();
            history.reverse();
            json!({ "pending": s.pending.values().collect::<Vec<_>>(), "history": history })
        })
        .await)
}

async fn approve(State(app): S, w: Who, Path(id): Path<Uuid>) -> J {
    w.require(&[ShariaBoard])?;
    let content = app
        .read(|e| e.state().pending.get(&id).map(|p| serde_json::to_vec(&p.proposal).expect("serializes")))
        .await
        .ok_or_else(|| ApiError::NotFound(format!("tasdiq kutayotgan tx {id}")))?;
    let approval = app.sign_approval(id, Role::ShariaBoard, &content);
    outcome(app.exec(move |e, now| Ok(e.approve(id, approval, now)?)).await?)
}

#[derive(Deserialize)]
struct Reason {
    reason: String,
}

async fn deny(State(app): S, w: Who, Path(id): Path<Uuid>, Json(b): Json<Reason>) -> J {
    w.require(&[ShariaBoard])?;
    outcome(app.exec(move |e, now| Ok(e.deny(id, w.0, b.reason, now)?)).await?)
}

// ------------------------------------------------------------------------------ ledger va audit

#[derive(Deserialize)]
struct LedgerQ {
    #[serde(default)]
    before: Option<u64>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn ledger(State(app): S, w: Who, Query(q): Query<LedgerQ>) -> J {
    w.require(&[ShariaBoard, Regulator, Admin, Operator])?;
    let limit = q.limit.unwrap_or(50).min(500);
    let entries = app.entries();
    let total = entries.len();
    let page: Vec<_> = entries
        .into_iter()
        .rev()
        .filter(|e| q.before.map(|b| e.seq < b).unwrap_or(true))
        .take(limit)
        .collect();
    ok(json!({ "total": total, "entries": page }))
}

async fn verify(State(app): S, w: Who) -> J {
    w.require(&[ShariaBoard, Regulator, Admin])?;
    ok(app.verify().await?)
}

async fn rejected(State(app): S, w: Who) -> J {
    w.require(&[ShariaBoard, Regulator, Admin, Operator])?;
    ok(app
        .read(|e| {
            let mut v = e.state().rejected.clone();
            v.reverse();
            v
        })
        .await)
}

#[derive(Deserialize)]
struct LotsQ {
    #[serde(default)]
    owner: Option<PartyId>,
    #[serde(default)]
    class: Option<MoneyClass>,
    #[serde(default)]
    all: Option<bool>,
}

async fn list_lots(State(app): S, w: Who, Query(q): Query<LotsQ>) -> J {
    let owner = match w.0 {
        Actor::Investor(me) => Some(me),
        _ => q.owner,
    };
    ok(app
        .read(|e| {
            let s = e.state();
            let mut v: Vec<Value> = s
                .lots
                .values()
                .filter(|l| owner.is_none() || l.owner == owner)
                .filter(|l| q.class.is_none() || Some(l.class) == q.class)
                .filter(|l| q.all.unwrap_or(false) || s.lot_status(l.id) != Some(LotStatus::Spent))
                .map(|l| json!({ "lot": l, "status": s.lot_status(l.id) }))
                .collect();
            v.sort_by_key(|x| std::cmp::Reverse(x["lot"]["seq"].as_u64().unwrap_or(0)));
            v.truncate(500);
            v
        })
        .await)
}

async fn lot_lineage(State(app): S, w: Who, Path(id): Path<Uuid>) -> J {
    let l = app.read(|e| trace::lineage(e.state(), id)).await;
    if let Actor::Investor(me) = w.0 {
        if l.first().and_then(|x| x.owner) != Some(me) {
            return Err(ApiError::Forbidden("faqat o'z lotlaringiz".into()));
        }
    }
    ok(l)
}

#[derive(Deserialize)]
struct TraceQ {
    #[serde(default)]
    hops: Option<u32>,
    #[serde(default)]
    days: Option<u32>,
}

async fn lot_trace(State(app): S, w: Who, Path(id): Path<Uuid>, Query(q): Query<TraceQ>) -> J {
    w.require(&[ShariaBoard, Admin, Regulator])?;
    ok(app.read(|e| trace::forward(e.state(), id, q.hops.unwrap_or(6), q.days.unwrap_or(365))).await)
}

#[derive(Deserialize)]
struct FreezeBody {
    lot_id: LotId,
    #[serde(default = "default_hops")]
    hop_limit: u32,
    #[serde(default = "default_days")]
    window_days: u32,
    reason: String,
}

fn default_hops() -> u32 {
    6
}

fn default_days() -> u32 {
    90
}

async fn freeze(State(app): S, w: Who, Json(b): Json<FreezeBody>) -> J {
    w.require(&[Admin, ShariaBoard])?;
    outcome(app.exec(move |e, now| Ok(e.freeze(b.lot_id, b.hop_limit, b.window_days, b.reason, w.0, now)?)).await?)
}

async fn unfreeze(State(app): S, w: Who, Path(id): Path<Uuid>, Json(b): Json<Reason>) -> J {
    w.require(&[Admin, ShariaBoard])?;
    outcome(app.exec(move |e, _| Ok(e.unfreeze(id, w.0, b.reason)?)).await?)
}

async fn reconcile(State(app): S, w: Who) -> J {
    w.require(&[Admin, ShariaBoard, Operator])?;
    app.process_outbox().await?;
    outcome(app.reconcile().await?)
}

async fn list_anchors(State(app): S, _w: Who) -> J {
    ok(app
        .read(|e| {
            let mut v = e.state().anchors.clone();
            v.reverse();
            v
        })
        .await)
}

#[derive(Deserialize)]
struct AnchorBody {
    #[serde(default)]
    day: Option<NaiveDate>,
}

async fn anchor(State(app): S, w: Who, Json(b): Json<AnchorBody>) -> J {
    w.require(&[Admin])?;
    let today = app.today();
    let day = b.day.unwrap_or(today - Duration::days(1));
    if day == today && !app.dev {
        return Err(ApiError::BadRequest("bugungi kun hali yopilmagan".into()));
    }
    if day > today {
        return Err(ApiError::BadRequest("kelajak sana".into()));
    }
    outcome(app.anchor(day).await?)
}

// ------------------------------------------------------------------------------ dashboard'lar

async fn investor_dashboard(State(app): S, w: Who, Path(id): Path<Uuid>) -> J {
    match w.0 {
        Actor::Investor(me) if me != id => return Err(ApiError::Forbidden("faqat o'z dashboard'ingiz".into())),
        Actor::Regulator => return Err(ApiError::Forbidden("regulyator faqat agregat ko'rsatkichlarni ko'radi".into())),
        _ => {}
    }
    let today = app.today();
    let d = app
        .read(|e| views::investor_dashboard(e.state(), id, today))
        .await
        .ok_or_else(|| ApiError::NotFound(format!("investor {id}")))?;
    ok(d)
}

async fn auditor(State(app): S, w: Who) -> J {
    w.require(&[ShariaBoard, Admin])?;
    let v = app.read(|e| views::auditor_view(e.state())).await;
    ok(json!({ "view": v, "integrity_error": app.integrity_error(), "bank_balance": app.bank.balance() }))
}

async fn regulator(State(app): S, w: Who) -> J {
    w.require(&[Regulator, ShariaBoard, Admin])?;
    let today = app.today();
    ok(app.read(|e| views::regulator_view(e.state(), today)).await)
}

async fn operator(State(app): S, w: Who) -> J {
    w.require(&[Operator, Admin])?;
    let today = app.today();
    let outbox = app.outbox().await?;
    let v = app
        .read(|e| {
            let s = e.state();
            let mut contracts: Vec<_> = s.contracts.values().map(|c| views::contract_view(s, c, today)).collect();
            contracts.sort_by_key(|c| std::cmp::Reverse(c.contract.created_at));
            let mut rejected = s.rejected.clone();
            rejected.reverse();
            rejected.truncate(50);
            json!({
                "balances": views::class_balances(s),
                "contracts": contracts,
                "pending": s.pending.values().collect::<Vec<_>>(),
                "rejected": rejected,
                "txs": views::recent_txs(s, 30),
            })
        })
        .await;
    let mut v = v;
    v["outbox"] = serde_json::to_value(outbox).unwrap_or_default();
    ok(v)
}

async fn bank(State(app): S, w: Who) -> J {
    w.require(&[Operator, ShariaBoard, Regulator, Admin])?;
    let mut statement = app.bank.statement();
    statement.reverse();
    statement.truncate(200);
    ok(json!({ "balance": app.bank.balance(), "statement": statement, "rejected_orders": app.bank.rejected_orders() }))
}

// ------------------------------------------------------------------------------ dev

#[derive(Deserialize)]
struct IncomingBody {
    kind: IncomingKind,
    party: PartyId,
    amount: Money,
    #[serde(default)]
    contract_id: Option<ContractId>,
    #[serde(default)]
    late_fee: Option<Money>,
}

async fn dev_incoming(State(app): S, _w: Who, Json(b): Json<IncomingBody>) -> J {
    let (o, bank_ref) =
        app.ingest_incoming(b.kind, b.party, b.amount, b.contract_id, b.late_fee.unwrap_or(Money::ZERO)).await?;
    ok(json!({ "outcome": o, "bank_ref": bank_ref }))
}

#[derive(Deserialize)]
struct AdjustBody {
    delta: Money,
    memo: String,
}

async fn dev_adjust(State(app): S, w: Who, Json(b): Json<AdjustBody>) -> J {
    w.require(&[Admin])?;
    app.bank.adjust(b.delta, &b.memo, app.now());
    ok(json!({ "balance": app.bank.balance() }))
}

async fn dev_decline_next(State(app): S, w: Who, Json(b): Json<Reason>) -> J {
    w.require(&[Admin])?;
    app.bank.decline_next(&b.reason);
    ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
struct TamperBody {
    seq: u64,
    from: String,
    to: String,
}

async fn dev_tamper(State(app): S, w: Who, Json(b): Json<TamperBody>) -> J {
    w.require(&[Admin])?;
    app.store.tamper(b.seq, &b.from, &b.to).await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
    ok(app.verify().await?)
}

#[derive(Deserialize)]
struct ClockBody {
    days: i64,
}

async fn dev_clock(State(app): S, w: Who, Json(b): Json<ClockBody>) -> J {
    w.require(&[Admin])?;
    let offset = app.advance_clock(b.days)?;
    ok(json!({ "offset_days": offset, "today": app.today() }))
}

async fn dev_seed(State(app): S, w: Who) -> J {
    w.require(&[Admin])?;
    ok(crate::seed::seed(&app).await?)
}
