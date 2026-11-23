# invest4f — invest for future

Shariatga mos investitsiya fondlari uchun **enforcement ledger** (ishchi nomi — **Omonat**).

> Detection emas — enforcement. Har bir chiquvchi to'lov bajarilishidan **oldin** shariat va shartnoma qoidalariga tekshiriladi. Qoidaga zid to'lovni bank bajarmaydi.

To'liq dizayn: [`plan.md`](plan.md).

## Muammo

Investor puli operator hisobida turadi, kitobni ham, investor ko'radigan hisobotni ham operatorning o'zi yuritadi. Natijada quyidagi holatlar oylar o'tib, post-factum aniqlanadi:

- dividend hali tushmagan puldan to'lanadi;
- boshqaruv xarajatlari investor kapitalidan qilinadi;
- rezerv shartnomadagidan ko'p ushlanadi;
- investorga boshqa investorning pulidan to'lanadi (Ponzi pattern).

## G'oya

Har bir pul bo'lagi (**lot**) o'z egasi, klassi va to'liq tarixi bilan yuritiladi. Operator to'lovni faqat **taklif** qiladi. Policy Engine uni invariantlarga tekshiradi, kerak bo'lsa Sharia board imzosini kutadi. Bankka to'lov topshirig'i faqat **operator + Omonat (HSM)** imzosi bilan chiqadi (2-of-2 mandat).

Model: **mudaraba fondi + murabaha**. Investorlar fondga pul kiritadi, mudarib tovar sotib olib mijozga ustama bilan muddatli to'lovga sotadi. Asosiy qism kapitalga qaytadi, ustama esa foyda sifatida rezerv, mudarib ulushi va investorlar o'rtasida taqsimlanadi.

## Invariantlar

| ID | Qoida | Buzilsa |
|---|---|---|
| I1 | `Dividend` faqat `InvestorProfit` lotlaridan, Owner = oluvchi | Reject |
| I2 | `Withdrawal` faqat oluvchining `Capital` lotlaridan; turli egalarning lotlari aralashmaydi | Reject |
| I3 | Operatsion xarajat faqat `MudaribShare` dan | Reject |
| I4 | `Deploy` faqat `Capital` dan; shartnoma tovar xaridi va egalik hujjatisiz `Active` bo'lmaydi | Reject |
| I5 | Rezervga ajratish ≤ policy cap | Reject |
| I6 | `MudaribShare` = foydaning kelishilgan %, kapitaldan hech qachon | Reject |
| I7 | Kechikish jarimasi faqat `Charity` ga, `Charity` faqat tasdiqlangan oluvchiga | Reject |
| I8 | Related-party oluvchiga to'lov — operator + Sharia board imzosi | Pending |
| I9 | `ReserveUse` faqat Sharia board qarori bilan | Pending |
| I10 | To'lov va zarar pozitsiya ulushiga pro-rata | Reject |
| I11 | Ledger yig'indisi = bank qoldig'i (reconciliation) | Incident |
| I12 | Append-only, hash chain, kunlik Merkle root | Incident |
| I13 | Kafolatlangan daromad config'da ham, UI'da ham yo'q | Config validatsiyasi |

Rad etilgan har bir urinish ledger'ga yoziladi va auditor ko'rinishiga tushadi.

## Arxitektura

```
Operator / bank kirimi ─▶ TxProposal
                             │
                             ▼
                 Policy Engine (STRUCT, I1–I10, FLOW)
                    ok │        reject → Rejected log
                       ▼
                 Approval Service (I8, I9: Sharia board imzosi)
                       ▼
                 Ledger Core: hash chain + outbox (bitta tranzaksiyada)
                       ▼
                 Bank Gateway: operator + Omonat imzosi
                       ▼
                 Bank ─▶ PaymentConfirmed / PaymentFailed → Compensation
                       ▼
                 Reconciliation (I11) · kunlik Merkle root (I12)
```

| Qism | Vazifasi |
|---|---|
| `crates/omonat-core` | Domen mantig'i (IO'siz): lotlar va lineage, Policy Engine, murabaha shartnomalari, Allocation Engine, hash chain / Merkle root, tracing & freeze, dashboard ko'rinishlari |
| `crates/omonat-store` | Event store: PostgreSQL (append-only — trigger va revoke qilingan UPDATE/DELETE) yoki xotirada; transactional outbox |
| `crates/omonat-bank` | Imzo kaliti (HSM o'rnida Ed25519), payment order, 2-of-2 mandatni tekshiradigan mock bank |
| `crates/omonat-server` | HTTP API (axum), Bank Gateway worker, reconciliation, anchoring, dev simulyatori |
| `web` | React + Vite + TypeScript: investor, operator, Sharia auditor, regulyator va dev sahifalari |

Barcha summalar butun sonda, tiyinda saqlanadi. Pro-rata taqsimot qoldiq tiyinlarni ham aniq taqsimlaydi, shuning uchun yig'indilar hech qachon farq qilmaydi.

## Ishga tushirish

Talablar: Rust (stable), Node.js 22+, Docker (PostgreSQL uchun, ixtiyoriy).

### Tezkor (ledger xotirada)

```bash
cd web && npm install && npm run build && cd ..
OMONAT_DEV=1 cargo run -p omonat-server
```

http://127.0.0.1:8787 ni oching, **Admin / dev** sifatida kiring va **«Demo ma'lumotlarni yaratish»** tugmasini bosing. Investorlar, policy, depozitlar va shartnomalar oddiy yo'l bilan (bank kirimi → Policy Engine → outbox) yaratiladi.

Frontend'ni ishlab chiqish rejimida ishlatish: `cd web && npm run dev` (http://localhost:5173, `/api` serverga proksi qilinadi).

### PostgreSQL bilan

```bash
docker compose up -d postgres
DATABASE_URL=postgres://omonat_app:omonat_app_dev@127.0.0.1:5442/omonat \
DATABASE_OWNER_URL=postgres://omonat_owner:omonat_owner_dev@127.0.0.1:5442/omonat \
OMONAT_DEV=1 cargo run -p omonat-server
```

Migratsiya jadval egasi (`omonat_owner`) bilan bajariladi. Ilova esa `omonat_app` roli bilan ishlaydi — unda `ledger_entries` ustida faqat SELECT/INSERT huquqi bor. Parollar faqat lokal dev uchun.

### Muhit o'zgaruvchilari

| O'zgaruvchi | Ma'nosi | Standart |
|---|---|---|
| `OMONAT_BIND` | Server manzili | `127.0.0.1:8787` |
| `DATABASE_URL` | PostgreSQL (ilova roli); berilmasa ledger xotirada | — |
| `DATABASE_OWNER_URL` | Migratsiya uchun jadval egasi | `DATABASE_URL` |
| `OMONAT_DEV` | `1` — `/api/dev/*` (seed, bank simulyatori, vaqtni surish, I12 namoyishi) | o'chiq |
| `OMONAT_DATA_DIR` | Mock bank holati fayli | `data` |
| `OMONAT_RECONCILE_SECS` | Reconciliation davri | `86400` |
| `OMONAT_HSM_SEED`, `OMONAT_OPERATOR_SEED`, `OMONAT_SHARIA_SEED` | Imzo kalitlari seed'lari (berilmasa dev kalitlari) | dev |
| `OMONAT_WEB_DIR` | Frontend build papkasi | `web/dist` |

## Testlar

```bash
cargo test --workspace
```

Qabul mezonlari (plan.md §17) — har bir invariant test bilan isbotlanadi:

| Test | Tekshiruv | Joyi |
|---|---|---|
| T1 | Yangi investor kapitalidan dividend → rad (I1) | `crates/omonat-core/tests/acceptance.rs` |
| T2 | Investor A yechishi B lotlaridan → rad (I2) | 〃 |
| T3 | Marketing to'lovi `Capital` dan → rad (I3) | 〃 |
| T4 | Tovar sotib olinmagan shartnomani faollashtirish → rad (I4) | 〃 |
| T5 | Rezerv cap + 1 so'm → rad (I5) | 〃 |
| T6 | Kechikish jarimasini `Profit` ga yozish → rad (I7) | 〃 |
| T7 | Related-party to'lovi Sharia board imzosisiz bajarilmaydi (I8) | 〃 |
| T8 | Defolt → zarar pro-rata, yig'indi aniq mos (I10) | 〃 |
| T9 | Yozuv o'zgartirilsa hash chain / Merkle root buzilishi aniqlanadi (I12) | 〃 |
| T10 | Ledger ≠ bank qoldig'i → incident (I11) | 〃 |
| T11 | Dividend lineage'i shartnoma ustamasigacha yetib boradi | 〃 |
| T12 | Operator Omonat imzosisiz bankka to'lov yubora olmaydi | `crates/omonat-bank/src/mock.rs` |
| T13 | Likvidi yetmagan investorga faqat likvid qism, qolgani jadval bilan | `crates/omonat-core/tests/acceptance.rs` |

PostgreSQL'da UPDATE/DELETE bloklanishini tekshiradigan test faqat test bazasi berilganda ishlaydi:

```bash
OMONAT_TEST_DATABASE_URL=postgres://omonat_app:omonat_app_dev@127.0.0.1:5442/omonat_test \
OMONAT_TEST_DATABASE_OWNER_URL=postgres://omonat_owner:omonat_owner_dev@127.0.0.1:5442/omonat_test \
cargo test -p omonat-store
```

## Holati va cheklovlar

Bu — prototip. Quyidagilar hozircha production o'rnini bosuvchi dev yechimlar:

- **Bank** — mock bank. Haqiqiy hamkor bank integratsiyasi va 2-of-2 mandat hali yo'q.
- **HSM** — dasturiy Ed25519 kaliti (`Signer` interfeysi ortida).
- **Autentifikatsiya** — `X-Omonat-Actor` sarlavhasi. Server dev rejimida rollar nomidan imzo qo'yadi; production'da har bir rolning o'z kaliti bo'ladi.
- **Read model'lar** — ishga tushganda event store'dan xotirada quriladi.
- **OpenTelemetry** ulanmagan; alert'lar log va auditor ko'rinishida.
- Muzlatilgan pulni tekshiruvdan keyin qaytarish (reversal) hali qilinmagan.
- plan.md §18 dagi ochiq savollar (allocation limitlari, foyda taqsimoti tartibi va boshqalar) hali hal qilinmagan.

## Litsenziya

[GNU Affero General Public License v3.0](LICENSE) (`AGPL-3.0-only`).
