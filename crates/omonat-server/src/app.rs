//! Ilova yadrosi: yozish yo'li (policy → ledger → outbox), Bank Gateway, reconciliation, anchoring.

use crate::error::{ApiError, ApiResult};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use omonat_bank::{sign_order, MockBank, PaymentOrder, SignedOrder, Signer, SoftwareSigner};
use omonat_core::chain::{verify_chain, LedgerEntry};
use omonat_core::engine::{verify_anchors, AnchorCheck, Decision};
use omonat_core::model::*;
use omonat_core::{builders, local_date, Engine, Event, Money, Outcome};
use omonat_store::{EventStore, OutboxItem, OUTBOX_FAILED, OUTBOX_NEW, OUTBOX_SENT};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::{Mutex, Notify};

/// Rollarning imzo kalitlari. Omonat kaliti — HSM o'rnida (dev: dasturiy).
pub struct Keys {
    pub omonat: SoftwareSigner,
    pub operator: SoftwareSigner,
    pub sharia: SoftwareSigner,
}

pub struct App {
    engine: Mutex<Engine>,
    entries: RwLock<Vec<LedgerEntry>>,
    pub store: Arc<dyn EventStore>,
    pub bank: MockBank,
    pub keys: Keys,
    pub dev: bool,
    integrity: RwLock<Option<String>>,
    pub outbox_notify: Notify,
    /// Bank Gateway va reconciliation bir vaqtda ishlamasligi uchun.
    gateway: Mutex<()>,
    clock_offset_secs: AtomicI64,
}

#[derive(Serialize)]
pub struct VerifyReport {
    pub entries: usize,
    pub chain_ok: bool,
    pub chain_error: Option<String>,
    pub anchors: Vec<AnchorCheck>,
    pub store_matches_memory: bool,
}

#[derive(Copy, Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncomingKind {
    Deposit,
    Installment,
}

impl App {
    pub async fn start(store: Arc<dyn EventStore>, bank: MockBank, keys: Keys, dev: bool) -> anyhow::Result<Arc<App>> {
        let entries = store.load().await?;
        let (engine, integrity) = match Engine::replay(&entries) {
            Ok(e) => (e, None),
            Err(err) => {
                tracing::error!("ledger replay failed: {err}");
                (Engine::new(), Some(err.to_string()))
            }
        };
        tracing::info!(entries = entries.len(), store = store.kind(), "ledger loaded");
        Ok(Arc::new(App {
            engine: Mutex::new(engine),
            entries: RwLock::new(entries),
            store,
            bank,
            keys,
            dev,
            integrity: RwLock::new(integrity),
            outbox_notify: Notify::new(),
            gateway: Mutex::new(()),
            clock_offset_secs: AtomicI64::new(0),
        }))
    }

    pub fn now(&self) -> DateTime<Utc> {
        Utc::now() + Duration::seconds(self.clock_offset_secs.load(Ordering::Relaxed))
    }

    pub fn today(&self) -> NaiveDate {
        local_date(self.now())
    }

    /// Dev: vaqtni oldinga surish (muddati o'tgan to'lovlarni ko'rsatish uchun). Orqaga yo'q.
    pub fn advance_clock(&self, days: i64) -> ApiResult<i64> {
        if days <= 0 {
            return Err(ApiError::BadRequest("vaqt faqat oldinga suriladi".into()));
        }
        Ok(self.clock_offset_secs.fetch_add(days * 86_400, Ordering::Relaxed) / 86_400 + days)
    }

    pub fn clock_offset_days(&self) -> i64 {
        self.clock_offset_secs.load(Ordering::Relaxed) / 86_400
    }

    pub fn integrity_error(&self) -> Option<String> {
        self.integrity.read().unwrap().clone()
    }

    /// O'qish uchun holat.
    pub async fn read<T>(&self, f: impl FnOnce(&Engine) -> T) -> T {
        let e = self.engine.lock().await;
        f(&e)
    }

    pub fn entries(&self) -> Vec<LedgerEntry> {
        self.entries.read().unwrap().clone()
    }

    /// Yagona yozish yo'li: buyruq → qaror → hash chain yozuvlari + imzolangan payment
    /// order'lar bitta atomik amalda store'ga → so'ng holatga qo'llanadi.
    pub async fn exec<F>(&self, f: F) -> ApiResult<Outcome>
    where
        F: FnOnce(&Engine, DateTime<Utc>) -> ApiResult<Decision>,
    {
        if let Some(err) = self.integrity_error() {
            return Err(ApiError::Integrity(err));
        }
        let mut eng = self.engine.lock().await;
        let now = self.now();
        let decision = f(&eng, now)?;
        self.commit_locked(&mut eng, decision, now).await
    }

    async fn commit_locked(&self, eng: &mut Engine, d: Decision, now: DateTime<Utc>) -> ApiResult<Outcome> {
        if d.events.is_empty() {
            return Ok(d.outcome);
        }
        let entries = eng.prepare(&d.events, now);
        let mut outbox = Vec::new();
        for (ev, entry) in d.events.iter().zip(&entries) {
            if let Event::TxApplied { tx } = ev {
                for p in &tx.payments {
                    let party = eng.state().party(p.beneficiary).expect("validated beneficiary");
                    // Omonat (HSM) imzosi faqat shu yerda — policy'dan o'tgan tx uchun — qo'yiladi.
                    let signed = sign_order(PaymentOrder::from_payment(p, party), &self.keys.operator, &self.keys.omonat);
                    outbox.push(OutboxItem {
                        id: p.id,
                        ledger_seq: entry.seq,
                        body: serde_json::to_string(&signed).expect("serializes"),
                        status: OUTBOX_NEW.into(),
                        attempts: 0,
                        note: None,
                        created_at: now,
                    });
                }
            }
            if let Event::Rejected { attempt } = ev {
                let rules: Vec<&str> = attempt.violations.iter().map(|v| v.rule_id.as_str()).collect();
                tracing::warn!(rules = ?rules, "ALERT: policy rejected an attempt — auditor notified");
            }
            if let Event::IncidentOpened { incident } = ev {
                tracing::error!(kind = ?incident.kind, details = %incident.details, "ALERT: incident opened");
            }
        }
        self.store.append(&entries, &outbox).await?;
        eng.apply_entries(&entries);
        self.entries.write().unwrap().extend(entries);
        if !outbox.is_empty() {
            self.outbox_notify.notify_one();
        }
        Ok(d.outcome)
    }

    pub async fn propose(&self, p: TxProposal) -> ApiResult<Outcome> {
        self.exec(move |e, now| Ok(e.propose_tx(p, now)?)).await
    }

    /// Imzo: subject + mazmun xeshi ustidan.
    pub fn sign_approval(&self, subject: uuid::Uuid, role: Role, content: &[u8]) -> Approval {
        let signer: &dyn Signer = match role {
            Role::ShariaBoard => &self.keys.sharia,
            _ => &self.keys.operator,
        };
        let digest = hex::encode(Sha256::digest(content));
        let msg = format!("omonat.approval.v1|{subject}|{role:?}|{digest}");
        Approval {
            subject,
            role,
            signer: signer.key_id().to_string(),
            signature: signer.sign(msg.as_bytes()),
            public_key: signer.public_key_hex(),
            at: self.now(),
        }
    }

    // ------------------------------------------------------------------ Bank Gateway

    /// Outbox'dagi imzolangan topshiriqlarni bankka yuboradi; natijani ledger'ga yozadi.
    pub async fn process_outbox(&self) -> anyhow::Result<usize> {
        if self.integrity_error().is_some() {
            return Ok(0);
        }
        let _g = self.gateway.lock().await;
        let items = self.store.outbox_pending(50).await?;
        let mut done = 0;
        for item in items {
            let signed: SignedOrder = serde_json::from_str(&item.body)?;
            let result = self.bank.execute(&signed, self.now());
            let (status, note) = match &result {
                Ok(r) => (OUTBOX_SENT, Some(r.clone())),
                Err(e) => (OUTBOX_FAILED, Some(e.to_string())),
            };
            let id = item.id;
            let outcome = match result {
                Ok(bank_ref) => self.exec(move |e, _| Ok(e.payment_confirmed(id, bank_ref)?)).await,
                Err(err) => self.exec(move |e, now| Ok(e.payment_failed(id, err.to_string(), now)?)).await,
            };
            match outcome {
                Ok(_) | Err(ApiError::Conflict(_)) => {
                    self.store.outbox_mark(id, status, note).await?;
                    done += 1;
                }
                Err(e) => tracing::error!(payment = %id, "outbox: {e:?}"),
            }
        }
        Ok(done)
    }

    /// Bankka kirim (mock bank) va uni lot'ga aylantirish. Avval quruq tekshiruv: policy
    /// qabul qilmaydigan pul bankka ham tushirilmaydi (dev simulyatorida).
    pub async fn ingest_incoming(
        &self,
        kind: IncomingKind,
        party: PartyId,
        amount: Money,
        contract_id: Option<ContractId>,
        late_fee: Money,
    ) -> ApiResult<(Outcome, String)> {
        let build = |e: &Engine, bank_ref: &str| -> ApiResult<TxProposal> {
            Ok(match kind {
                IncomingKind::Deposit => {
                    if !e.state().is_party(party, PartyKind::Investor) {
                        return Err(ApiError::BadRequest("depozit faqat ro'yxatdagi investordan".into()));
                    }
                    builders::deposit(party, amount, bank_ref)
                }
                IncomingKind::Installment => {
                    let cid = contract_id.ok_or_else(|| ApiError::BadRequest("contract_id kerak".into()))?;
                    builders::installment(e.state(), cid, amount, late_fee, bank_ref)?
                }
            })
        };
        let (name, dry) = {
            let e = self.engine.lock().await;
            let p = build(&e, "DRY-RUN")?;
            let mut probe = p.clone();
            probe.bank_ref = Some(format!("dry-{}", uuid::Uuid::new_v4()));
            let d = e.propose_tx(probe, self.now())?;
            (e.state().party(party).map(|x| x.name.clone()).unwrap_or_default(), d.outcome)
        };
        if let Outcome::Rejected { violations } = dry {
            let msg = violations.iter().map(|v| format!("{}: {}", v.rule_id, v.reason)).collect::<Vec<_>>().join("; ");
            return Err(ApiError::BadRequest(format!("bank kirimni qabul qilmadi — {msg}")));
        }
        let memo = match kind {
            IncomingKind::Deposit => "Mudaraba depoziti".to_string(),
            IncomingKind::Installment => format!("Shartnoma to'lovi {}", contract_id.unwrap_or_default()),
        };
        let line = self.bank.incoming(&name, amount, &memo, self.now());
        let bank_ref = line.bank_ref.clone();
        let outcome = self
            .exec(move |e, now| {
                let p = build(e, &line.bank_ref)?;
                Ok(e.propose_tx(p, now)?)
            })
            .await?;
        Ok((outcome, bank_ref))
    }

    // ------------------------------------------------------------------ nazorat

    /// I11: ledger ↔ bank qoldig'i.
    pub async fn reconcile(&self) -> ApiResult<Outcome> {
        let _g = self.gateway.lock().await;
        let balance = self.bank.balance();
        self.exec(move |e, now| Ok(e.reconcile(balance, now))).await
    }

    /// Kunlik Merkle root (I12 anchoring).
    pub async fn anchor(&self, day: NaiveDate) -> ApiResult<Outcome> {
        let entries = self.entries();
        let omonat = &self.keys.omonat;
        self.exec(move |e, now| Ok(e.anchor(day, &entries, |m| (omonat.sign(m), omonat.public_key_hex()), now)?)).await
    }

    /// Store'dagi yozuvlarni qayta o'qib zanjir va anchor'larni tekshiradi. Buzilgan bo'lsa,
    /// yozish (va bankka to'lov yuborish) to'xtatiladi.
    pub async fn verify(&self) -> anyhow::Result<VerifyReport> {
        let stored = self.store.load().await?;
        let chain = verify_chain(&stored);
        let anchors = {
            let e = self.engine.lock().await;
            verify_anchors(&stored, &e.state().anchors)
        };
        let memory = self.entries();
        let store_matches_memory = stored == memory;
        let mut problem = chain.as_ref().err().map(|e| e.to_string());
        if problem.is_none() {
            if let Some(a) = anchors.iter().find(|a| !a.ok) {
                problem = Some(format!("{} kungi Merkle root e'lon qilinganiga mos emas", a.anchor.day));
            } else if !store_matches_memory {
                problem = Some("store'dagi yozuvlar ilova ko'rgan ledger'dan farq qiladi".into());
            }
        }
        if let Some(p) = &problem {
            tracing::error!("INCIDENT: ledger integrity: {p}");
            *self.integrity.write().unwrap() = Some(p.clone());
        }
        Ok(VerifyReport {
            entries: stored.len(),
            chain_ok: chain.is_ok(),
            chain_error: chain.err().map(|e| e.to_string()),
            anchors,
            store_matches_memory,
        })
    }

    pub async fn outbox(&self) -> anyhow::Result<Vec<OutboxItem>> {
        self.store.outbox_all(100).await
    }
}

/// Fon jarayonlari: Bank Gateway, kunlik reconciliation va anchoring.
pub fn spawn_workers(app: Arc<App>, reconcile_every: std::time::Duration) {
    let a = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = a.outbox_notify.notified() => {},
                _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {},
            }
            if let Err(e) = a.process_outbox().await {
                tracing::error!("bank gateway: {e:#}");
            }
        }
    });
    let a = app.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(reconcile_every);
        t.tick().await;
        loop {
            t.tick().await;
            if let Err(e) = a.reconcile().await {
                tracing::error!("reconciliation: {e:?}");
            }
        }
    });
    tokio::spawn(async move {
        let mut t = tokio::time::interval(std::time::Duration::from_secs(600));
        loop {
            t.tick().await;
            let yesterday = app.today() - Duration::days(1);
            let pending = app.read(|e| !e.state().anchors.iter().any(|x| x.day == yesterday)).await
                && app.entries().iter().any(|x| local_date(x.at) == yesterday);
            if pending {
                match app.anchor(yesterday).await {
                    Ok(_) => tracing::info!(%yesterday, "daily Merkle root anchored"),
                    Err(e) => tracing::error!("anchor: {e:?}"),
                }
            }
        }
    });
}
