-- Ilova roli: ledger_entries'ga faqat SELECT/INSERT (huquqlar migratsiyada beriladi).
-- Jadvallar egasi omonat_owner; server migratsiyani DATABASE_OWNER_URL bilan bajaradi.
CREATE ROLE omonat_app LOGIN PASSWORD 'omonat_app_dev';
GRANT CONNECT ON DATABASE omonat TO omonat_app;
GRANT USAGE ON SCHEMA public TO omonat_app;

-- Integratsiya testlari uchun alohida baza.
CREATE DATABASE omonat_test OWNER omonat_owner;
GRANT CONNECT ON DATABASE omonat_test TO omonat_app;
\c omonat_test
GRANT USAGE ON SCHEMA public TO omonat_app;
