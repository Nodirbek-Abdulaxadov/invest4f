-- Omonat event store. Ledger append-only (I12): UPDATE/DELETE/TRUNCATE huquqlari olib
-- tashlangan va trigger bilan bloklangan; tuzatish faqat compensating tx bilan.

CREATE TABLE IF NOT EXISTS ledger_entries (
    seq        BIGINT PRIMARY KEY CHECK (seq > 0),
    at         TIMESTAMPTZ NOT NULL,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL,          -- xeshlangan JSON aynan shu ko'rinishda saqlanadi
    prev_hash  BYTEA NOT NULL,
    hash       BYTEA NOT NULL UNIQUE
);

CREATE OR REPLACE FUNCTION omonat_forbid_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'ledger_entries append-only (I12): % taqiqlangan', TG_OP;
END
$$;

CREATE OR REPLACE TRIGGER ledger_entries_no_update
    BEFORE UPDATE OR DELETE ON ledger_entries
    FOR EACH ROW EXECUTE FUNCTION omonat_forbid_mutation();

CREATE OR REPLACE TRIGGER ledger_entries_no_truncate
    BEFORE TRUNCATE ON ledger_entries
    FOR EACH STATEMENT EXECUTE FUNCTION omonat_forbid_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON ledger_entries FROM PUBLIC;

-- Bank Gateway outbox: ledger yozuvi bilan bitta tranzaksiyada yoziladi.
CREATE TABLE IF NOT EXISTS outbox (
    id          UUID PRIMARY KEY,       -- payment_id
    ledger_seq  BIGINT NOT NULL REFERENCES ledger_entries(seq),
    body        TEXT NOT NULL,          -- imzolangan payment order (JSON)
    status      TEXT NOT NULL DEFAULT 'new',
    attempts    INT NOT NULL DEFAULT 0,
    note        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS outbox_status_idx ON outbox (status, created_at);

-- Ilova roli (agar mavjud bo'lsa) faqat o'qiydi va qo'shadi.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'omonat_app') THEN
        GRANT SELECT, INSERT ON ledger_entries TO omonat_app;
        REVOKE UPDATE, DELETE, TRUNCATE ON ledger_entries FROM omonat_app;
        GRANT SELECT, INSERT, UPDATE ON outbox TO omonat_app;
    END IF;
END
$$;
