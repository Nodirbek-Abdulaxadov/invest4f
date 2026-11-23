//! Omonat server: API, Bank Gateway (outbox), reconciliation, kunlik anchoring.

mod api;
mod app;
mod error;
mod seed;

use anyhow::Context;
use omonat_bank::mock::Mandate;
use omonat_bank::{MockBank, Signer, SoftwareSigner};
use omonat_store::{EventStore, MemoryStore, PgStore};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn key(name: &str, env_key: &str) -> SoftwareSigner {
    let seed = env(env_key).unwrap_or_else(|| {
        tracing::warn!("{env_key} berilmagan — dev kaliti ishlatilmoqda (production uchun emas)");
        format!("omonat-dev-{name}")
    });
    SoftwareSigner::from_seed(name, seed.as_bytes())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn")))
        .init();

    let dev = matches!(env("OMONAT_DEV").as_deref(), Some("1" | "true"));
    let bind = env("OMONAT_BIND").unwrap_or_else(|| "127.0.0.1:8787".into());
    let data_dir = PathBuf::from(env("OMONAT_DATA_DIR").unwrap_or_else(|| "data".into()));
    let reconcile_secs: u64 = env("OMONAT_RECONCILE_SECS").and_then(|v| v.parse().ok()).unwrap_or(86_400);

    let keys = app::Keys {
        omonat: key("omonat-hsm", "OMONAT_HSM_SEED"),
        operator: key("operator", "OMONAT_OPERATOR_SEED"),
        sharia: key("sharia-board", "OMONAT_SHARIA_SEED"),
    };
    let mandate = Mandate { operator_public_key: keys.operator.public_key_hex(), omonat_public_key: keys.omonat.public_key_hex() };

    let (store, bank): (Arc<dyn EventStore>, MockBank) = match env("DATABASE_URL") {
        Some(url) => {
            let owner = env("DATABASE_OWNER_URL");
            let store = PgStore::connect(&url, owner.as_deref()).await?;
            let bank = MockBank::persistent(mandate, data_dir.join("mock-bank.json")).context("mock bank holati")?;
            (Arc::new(store), bank)
        }
        None => {
            tracing::warn!("DATABASE_URL berilmagan — ledger xotirada (qayta ishga tushganda o'chadi)");
            (Arc::new(MemoryStore::new()), MockBank::new(mandate))
        }
    };

    let app = app::App::start(store, bank, keys, dev).await?;
    if let Some(err) = app.integrity_error() {
        tracing::error!("INCIDENT: ledger integrity broken at startup, writes disabled: {err}");
    }
    app::spawn_workers(app.clone(), std::time::Duration::from_secs(reconcile_secs));

    let mut router = api::router(app.clone()).layer(TraceLayer::new_for_http());
    if dev {
        router = router.layer(CorsLayer::permissive());
    }
    let web = PathBuf::from(env("OMONAT_WEB_DIR").unwrap_or_else(|| "web/dist".into()));
    if web.join("index.html").exists() {
        router = router.fallback_service(ServeDir::new(&web).fallback(ServeFile::new(web.join("index.html"))));
    }

    let listener = tokio::net::TcpListener::bind(&bind).await.with_context(|| format!("bind {bind}"))?;
    tracing::info!(%bind, dev, "Omonat server ishga tushdi");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
