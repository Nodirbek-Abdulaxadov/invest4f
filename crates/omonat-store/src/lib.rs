//! Event store: PostgreSQL (append-only) yoki xotirada (test/dev). Outbox ledger yozuvi bilan
//! bitta atomik amalda yoziladi.

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use omonat_core::chain::{decode_hash, LedgerEntry};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use std::sync::Mutex;
use uuid::Uuid;

pub const MIGRATION: &str = include_str!("../migrations/0001_ledger.sql");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxItem {
    pub id: Uuid,
    pub ledger_seq: u64,
    /// Imzolangan payment order (JSON).
    pub body: String,
    pub status: String,
    pub attempts: i32,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub const OUTBOX_NEW: &str = "new";
pub const OUTBOX_SENT: &str = "sent";
pub const OUTBOX_FAILED: &str = "failed";

#[async_trait]
pub trait EventStore: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn load(&self) -> Result<Vec<LedgerEntry>>;
    /// Yozuvlar mavjud zanjirni aynan davom ettirishi kerak; aks holda xato (fork yo'q).
    async fn append(&self, entries: &[LedgerEntry], outbox: &[OutboxItem]) -> Result<()>;
    async fn outbox_pending(&self, limit: i64) -> Result<Vec<OutboxItem>>;
    async fn outbox_all(&self, limit: i64) -> Result<Vec<OutboxItem>>;
    async fn outbox_mark(&self, id: Uuid, status: &str, note: Option<String>) -> Result<()>;
    /// Faqat dev/namoyish: yozuvni "qo'lda" o'zgartirish (I12 buzilishini ko'rsatish uchun).
    async fn tamper(&self, _seq: u64, _from: &str, _to: &str) -> Result<()> {
        bail!("bu store'da ledger yozuvini o'zgartirib bo'lmaydi (append-only)")
    }
}

fn check_continuity(head: Option<(u64, String)>, entries: &[LedgerEntry]) -> Result<()> {
    let Some(first) = entries.first() else { return Ok(()) };
    let (head_seq, head_hash) = head.unwrap_or((0, hex::encode(omonat_core::chain::GENESIS_HASH)));
    if first.seq != head_seq + 1 || first.prev_hash != head_hash {
        bail!("zanjir boshqa yozuv bilan davom etgan (kutilgan seq {}, kelgan {})", head_seq + 1, first.seq);
    }
    for w in entries.windows(2) {
        if w[1].seq != w[0].seq + 1 || w[1].prev_hash != w[0].hash {
            bail!("yozuvlar ketma-ket emas");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------ memory

#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<Vec<LedgerEntry>>,
    outbox: Mutex<Vec<OutboxItem>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl EventStore for MemoryStore {
    fn kind(&self) -> &'static str {
        "memory"
    }

    async fn load(&self) -> Result<Vec<LedgerEntry>> {
        Ok(self.entries.lock().unwrap().clone())
    }

    async fn append(&self, entries: &[LedgerEntry], outbox: &[OutboxItem]) -> Result<()> {
        let mut e = self.entries.lock().unwrap();
        let head = e.last().map(|x| (x.seq, x.hash.clone()));
        check_continuity(head, entries)?;
        e.extend_from_slice(entries);
        self.outbox.lock().unwrap().extend_from_slice(outbox);
        Ok(())
    }

    async fn outbox_pending(&self, limit: i64) -> Result<Vec<OutboxItem>> {
        Ok(self.outbox.lock().unwrap().iter().filter(|o| o.status == OUTBOX_NEW).take(limit as usize).cloned().collect())
    }

    async fn outbox_all(&self, limit: i64) -> Result<Vec<OutboxItem>> {
        Ok(self.outbox.lock().unwrap().iter().rev().take(limit as usize).cloned().collect())
    }

    async fn outbox_mark(&self, id: Uuid, status: &str, note: Option<String>) -> Result<()> {
        let mut o = self.outbox.lock().unwrap();
        let item = o.iter_mut().find(|x| x.id == id).ok_or_else(|| anyhow!("outbox {id} topilmadi"))?;
        item.status = status.to_string();
        item.attempts += 1;
        item.note = note;
        Ok(())
    }

    async fn tamper(&self, seq: u64, from: &str, to: &str) -> Result<()> {
        let mut e = self.entries.lock().unwrap();
        let entry = e.iter_mut().find(|x| x.seq == seq).ok_or_else(|| anyhow!("seq {seq} topilmadi"))?;
        if !entry.payload.contains(from) {
            bail!("seq {seq} yozuvida \"{from}\" yo'q");
        }
        entry.payload = entry.payload.replacen(from, to, 1);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------- postgres

pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    /// `owner_url` berilsa, migratsiya shu (egasi) roli bilan bajariladi; ilova esa `app_url`
    /// roli bilan ishlaydi — unda UPDATE/DELETE huquqi yo'q.
    pub async fn connect(app_url: &str, owner_url: Option<&str>) -> Result<Self> {
        let migrate_url = owner_url.unwrap_or(app_url);
        let owner = PgPoolOptions::new().max_connections(1).connect(migrate_url).await.context("PostgreSQL'ga ulanib bo'lmadi")?;
        sqlx::raw_sql(MIGRATION).execute(&owner).await.context("migratsiya")?;
        owner.close().await;
        let pool = PgPoolOptions::new().max_connections(8).connect(app_url).await.context("PostgreSQL'ga ulanib bo'lmadi")?;
        Ok(PgStore { pool })
    }

    fn outbox_row(r: &sqlx::postgres::PgRow) -> Result<OutboxItem> {
        Ok(OutboxItem {
            id: r.try_get("id")?,
            ledger_seq: r.try_get::<i64, _>("ledger_seq")? as u64,
            body: r.try_get("body")?,
            status: r.try_get("status")?,
            attempts: r.try_get("attempts")?,
            note: r.try_get("note")?,
            created_at: r.try_get("created_at")?,
        })
    }
}

#[async_trait]
impl EventStore for PgStore {
    fn kind(&self) -> &'static str {
        "postgres"
    }

    async fn load(&self) -> Result<Vec<LedgerEntry>> {
        let rows = sqlx::query("SELECT seq, at, kind, payload, prev_hash, hash FROM ledger_entries ORDER BY seq")
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| {
                Ok(LedgerEntry {
                    seq: r.try_get::<i64, _>("seq")? as u64,
                    at: r.try_get("at")?,
                    kind: r.try_get("kind")?,
                    payload: r.try_get("payload")?,
                    prev_hash: hex::encode(r.try_get::<Vec<u8>, _>("prev_hash")?),
                    hash: hex::encode(r.try_get::<Vec<u8>, _>("hash")?),
                })
            })
            .collect()
    }

    async fn append(&self, entries: &[LedgerEntry], outbox: &[OutboxItem]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;
        let head = sqlx::query("SELECT seq, hash FROM ledger_entries ORDER BY seq DESC LIMIT 1")
            .fetch_optional(&mut *tx)
            .await?
            .map(|r| -> Result<(u64, String)> {
                Ok((r.try_get::<i64, _>("seq")? as u64, hex::encode(r.try_get::<Vec<u8>, _>("hash")?)))
            })
            .transpose()?;
        check_continuity(head, entries)?;
        for e in entries {
            let prev = decode_hash(&e.prev_hash).ok_or_else(|| anyhow!("prev_hash"))?;
            let hash = decode_hash(&e.hash).ok_or_else(|| anyhow!("hash"))?;
            sqlx::query("INSERT INTO ledger_entries (seq, at, kind, payload, prev_hash, hash) VALUES ($1, $2, $3, $4, $5, $6)")
                .bind(e.seq as i64)
                .bind(e.at)
                .bind(&e.kind)
                .bind(&e.payload)
                .bind(prev.to_vec())
                .bind(hash.to_vec())
                .execute(&mut *tx)
                .await?;
        }
        for o in outbox {
            sqlx::query("INSERT INTO outbox (id, ledger_seq, body, status, created_at) VALUES ($1, $2, $3, $4, $5)")
                .bind(o.id)
                .bind(o.ledger_seq as i64)
                .bind(&o.body)
                .bind(&o.status)
                .bind(o.created_at)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn outbox_pending(&self, limit: i64) -> Result<Vec<OutboxItem>> {
        let rows = sqlx::query("SELECT * FROM outbox WHERE status = $1 ORDER BY created_at LIMIT $2")
            .bind(OUTBOX_NEW)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(Self::outbox_row).collect()
    }

    async fn outbox_all(&self, limit: i64) -> Result<Vec<OutboxItem>> {
        let rows = sqlx::query("SELECT * FROM outbox ORDER BY created_at DESC LIMIT $1").bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(Self::outbox_row).collect()
    }

    async fn outbox_mark(&self, id: Uuid, status: &str, note: Option<String>) -> Result<()> {
        sqlx::query("UPDATE outbox SET status = $2, note = $3, attempts = attempts + 1, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(status)
            .bind(note)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omonat_core::chain::make_entry;
    use omonat_core::event::Event;
    use omonat_core::model::{Actor, Party, PartyKind};

    fn ev(name: &str) -> Event {
        Event::PartyRegistered {
            party: Party { id: Uuid::new_v4(), kind: PartyKind::Investor, name: name.into(), bank_account: "1".into() },
            by: Actor::Operator,
        }
    }

    #[tokio::test]
    async fn memory_store_rejects_forks() {
        let s = MemoryStore::new();
        let e1 = make_entry(&omonat_core::chain::GENESIS_HASH, 1, Utc::now(), &ev("a"));
        s.append(&[e1.clone()], &[]).await.unwrap();
        let fork = make_entry(&omonat_core::chain::GENESIS_HASH, 1, Utc::now(), &ev("b"));
        assert!(s.append(&[fork], &[]).await.is_err());
        let e2 = make_entry(&decode_hash(&e1.hash).unwrap(), 2, Utc::now(), &ev("c"));
        s.append(&[e2], &[]).await.unwrap();
        assert_eq!(s.load().await.unwrap().len(), 2);
    }

    /// `OMONAT_TEST_DATABASE_URL` (ilova roli) va ixtiyoriy `OMONAT_TEST_DATABASE_OWNER_URL`
    /// berilganda: haqiqiy PostgreSQL'da UPDATE/DELETE ilova roli uchun ham, jadval egasi uchun ham bloklanadi.
    #[tokio::test]
    async fn postgres_blocks_update_and_delete() {
        let Ok(url) = std::env::var("OMONAT_TEST_DATABASE_URL") else { return };
        let owner_url = std::env::var("OMONAT_TEST_DATABASE_OWNER_URL").ok();
        let s = PgStore::connect(&url, owner_url.as_deref()).await.unwrap();
        if let Some(o) = &owner_url {
            let owner = PgPool::connect(o).await.unwrap();
            let upd = sqlx::query("UPDATE ledger_entries SET payload = payload WHERE seq = 1").execute(&owner).await;
            let has_rows = !s.load().await.unwrap().is_empty();
            assert!(!has_rows || upd.is_err(), "trigger egasining UPDATE'ini ham bloklashi kerak");
        }
        let head = s.load().await.unwrap();
        let (seq, prev) = head.last().map(|e| (e.seq, decode_hash(&e.hash).unwrap())).unwrap_or((0, omonat_core::chain::GENESIS_HASH));
        let e = make_entry(&prev, seq + 1, Utc::now(), &ev("pg"));
        s.append(&[e.clone()], &[]).await.unwrap();
        let upd = sqlx::query("UPDATE ledger_entries SET payload = 'x' WHERE seq = $1").bind(e.seq as i64).execute(&s.pool).await;
        assert!(upd.is_err());
        let del = sqlx::query("DELETE FROM ledger_entries WHERE seq = $1").bind(e.seq as i64).execute(&s.pool).await;
        assert!(del.is_err());
    }
}
