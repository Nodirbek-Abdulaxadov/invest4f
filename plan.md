# Omonat — shariatga mos investitsiya fondlari uchun enforcement ledger

> Ishchi nom. Hujjatda g'oya to'liq mos tushadigan holat va bitta yaxlit ish hajmi tasvirlangan.

---

## 1. Muammo

- Investor puli operatorning hisobida turadi va kitobni ham operatorning o'zi yuritadi.
- Investor ko'radigan hisobotni (dashboard) ham operator tayyorlaydi, uni tashqaridan tekshirib bo'lmaydi.
- Sharia auditor ma'lumotni kech va tanlab oladi.
- Natijada quyidagi holatlar post-factum, oylar o'tib aniqlanadi:
  - dividend hali tushmagan puldan to'lanadi;
  - boshqaruv xarajatlari investor kapitalidan qilinadi;
  - rezerv shartnomadagidan ko'p ushlanadi;
  - investorga boshqa investorning pulidan to'lanadi (Ponzi pattern).
- Kontekst: 2026-yil oktabrida IMAN Invest atrofidagi mojaroda aynan shu turdagi da'volar chiqdi (rasmiy xulosa hali yo'q).

## 2. G'oya bir gapda

Har bir pul bo'lagi (**lot**) o'z egasi, klassi va to'liq tarixi bilan yuritiladi. Har bir chiquvchi to'lov bajarilishidan **oldin** shariat va shartnoma qoidalariga tekshiriladi. Qoidaga zid to'lovni bank bajarmaydi.

**Detection emas — enforcement.**

---

## 3. To'liq mos holat (target scenario)

### 3.1. Ishtirokchilar

| Rol | Kim | Tizimdagi o'rni |
|---|---|---|
| Rabb al-mal | Investorlar (jismoniy va yuridik shaxslar) | Lot egalari, dashboard'ni o'qiydi |
| Mudarib | Fondni boshqaruvchi: islomiy bank oynasi yoki litsenziyali investitsiya kompaniyasi | To'lov taklif qiladi, lekin yolg'iz bajara olmaydi |
| Hamkor bank | Nominal hisob ochilgan bank | To'lovni faqat Omonat imzosi bilan bajaradi |
| Sharia board | Shariat kengashi / auditor | Policy'ni tasdiqlaydi, maxsus o'tkazmalarga ikkinchi imzo qo'yadi, real-time audit qiladi |
| Merchant | Tovar sotuvchi | Tovar puli oluvchi |
| Mijoz | Tovarni muddatli to'lovga oluvchi | Murabaha to'lovchisi |
| Regulyator | Markaziy bank | Read-only kirish |

### 3.2. Biznes modeli: mudaraba fondi + murabaha

1. Investorlar pulni fondga mudaraba asosida kiritadi.
2. Mudarib shu puldan merchantdan tovar sotib oladi. Tovarga egalik mudaribga o'tadi.
3. Mudarib tovarni mijozga tannarx + ustama narxida, muddatli to'lovga sotadi.
4. Mijoz to'laydi: **asosiy qism** kapitalga qaytadi, **ustama** foyda bo'ladi.
5. Foyda policy'ga ko'ra taqsimlanadi: rezerv (cap bilan), mudarib ulushi (kelishilgan %), qolgani investorlarga. Aniq tartib va foizlarni Sharia board tasdiqlaydi.
6. Defolt bo'lsa, zarar shu shartnomadagi pozitsiyalarga mutanosib taqsimlanadi. Zarar mudaribning aybi (ta'addi/taqsir) bilan bo'lsa, mudarib zimmasiga tushadi.

### 3.3. Bitta investorning yo'li

Aziz fondga 10 000 000 so'm kiritadi.

| Vaqt | Hodisa | Aziz dashboard'ida |
|---|---|---|
| Kun 0 | `Capital` lot, 10 mln, Owner = Aziz | Likvid: 10 mln |
| Kun 1 | Allocation engine bo'sh kapitalni yangi shartnomalarga taqsimlaydi: 40 ta shartnoma, har biriga ~250k | Faol: 10 mln, 40 ta pozitsiya |
| Oy 1 | Mijozlar to'laydi. Asosiy qism `Capital(Aziz)` ga qaytadi, ustama rezerv va mudarib ulushidan keyin `InvestorProfit(Aziz)` ga o'tadi | Qaytgan asosiy qism, olingan foyda, kutilayotgan foyda ("kafolatlanmagan" belgisi bilan) |
| Oy 3 | 1 ta shartnoma defolt bo'ladi. Zarar shu shartnoma egalariga pro-rata, Azizniki 250k doirasida. Rezervdan qoplash faqat Sharia board qarori bilan | Muammoli: 1 ta shartnoma, summa va holati |
| Oy 4 | Aziz 3 mln yechmoqchi, lekin likvidi 1.2 mln | Tizim 1.2 mln beradi va qolgani qachon bo'shashini to'lov jadvali bo'yicha ko'rsatadi. Boshqa investorning pulidan to'lab bo'lmaydi |
| Doim | Har bir dividend ustiga bosilsa | Dividend qaysi shartnomalarning qaysi oydagi ustamasidan kelgani ko'rinadi (lineage) |

### 3.4. Tizim rad etadigan urinishlar

| Urinish | Natija |
|---|---|
| Yangi investor kapitalidan eski investorlarga "dividend" | **I1** bo'yicha rad |
| Investor A ning yechib olishi investor B lotlaridan | **I2** bo'yicha rad |
| Marketing agentligiga to'lov `Capital` dan | **I3** bo'yicha rad |
| Tovar sotib olinmasdan mijoz shartnomasini faollashtirish | **I4** bo'yicha rad |
| Rezervga foydaning 50% i (cap 5%) | **I5** bo'yicha rad |
| Kechikish jarimasini foydaga qo'shish | **I7** bo'yicha rad |
| Rahbar bilan bog'liq kompaniyaga o'tkazma | **I8**: Sharia board imzosisiz bajarilmaydi |

Rad etilgan har bir urinish log'ga yoziladi va auditorga alert yuboriladi. Rad etilgan urinishning o'zi ham signal.

---

## 4. Shariat tamoyillari → tizim qoidalari

| Tamoyil | Tizimdagi ifodasi |
|---|---|
| Investor puli omonat | Lot egasi faqat explicit tx orqali o'zgaradi. Turli egalarning lotlari hech qachon aralashmaydi |
| Foyda faqat real faoliyatdan | Dividend faqat realized ustamadan (I1) |
| Kafolatlangan daromad yo'q | Product config'da fixed return maydoni yo'q. Kutilayotgan foyda "kafolatlanmagan" deb ko'rsatiladi (I13) |
| Zarar kapitalga mutanosib | Pro-rata loss allocation (I10) |
| Mudarib o'z ulushidan xarajat qiladi | Operatsion xarajat faqat `MudaribShare` dan (I3, I6) |
| Murabahada sotishdan oldin egalik | Ownership-before-sale (I4) |
| Kechikish jarimasi daromad emas | Faqat `Charity` ga (I7) |
| G'arar (noaniqlik) bo'lmasligi | Investor to'liq holatni real-time ko'radi |
| Manfaatlar to'qnashuvi bo'lmasligi | Related party uchun 2-of-2 imzo (I8) |

---

## 5. Pul klasslari va oqim

```
Investor ──Deposit──▶ [Capital(Owner)]
                          │ Deploy (tovar xaridi, merchantga)
                          ▼
                    Shartnoma + Position(investor, ulush)
                          │ mijozning muddatli to'lovi
                          ▼
        ┌── asosiy qism ──▶ [Capital(Owner)]   pro-rata pozitsiyaga
        ├── ustama ───────▶ [Profit(Contract)]
        │                       ├─▶ [Reserve]          ≤ cap
        │                       ├─▶ [MudaribShare]     kelishilgan %
        │                       └─▶ [InvestorProfit(Owner)] pro-rata
        └── kechikish jarimasi ─▶ [Charity]

[InvestorProfit(X)] ──Dividend──▶ X
[Capital(X)]        ──Withdrawal─▶ X
[MudaribShare]      ──Expense───▶ maosh, marketing, ofis
[Reserve]           ──ReserveUse─▶ defolt zararini qoplash (Sharia board qarori)
[Charity]           ──Donation──▶ tasdiqlangan xayriya tashkiloti

TAQIQ: Capital→Dividend · Capital→Expense · boshqa Owner lotidan to'lov
       · Reserve > cap · Charity→istalgan boshqa joy
```

---

## 6. Invariantlar (rule engine)

| ID | Qoida | Buzilsa |
|---|---|---|
| I1 | `Dividend` faqat `InvestorProfit` lotlaridan, Owner = oluvchi | Reject |
| I2 | `Withdrawal` faqat `Capital` lotlaridan, Owner = oluvchi | Reject |
| I3 | Operatsion xarajat (`Expense`) faqat `MudaribShare` dan | Reject |
| I4 | `Deploy` faqat `Capital` dan. Shartnoma `Active` ga o'tishi uchun tovar xaridi tx'i va egalik hujjati (qabz) bo'lishi shart | Reject |
| I5 | Davr bo'yicha rezervga ajratish ≤ policy cap (% foydadan) | Reject |
| I6 | `MudaribShare` = foydaning kelishilgan %, kapitaldan hech qachon | Reject |
| I7 | Kechikish jarimasi faqat `Charity` ga, `Charity` faqat tasdiqlangan oluvchiga | Reject |
| I8 | Related-party ro'yxatidagi oluvchiga to'lov uchun operator + Sharia board imzosi (2-of-2) | Pending |
| I9 | `ReserveUse` faqat Sharia board approval bilan | Pending |
| I10 | Zarar pozitsiya ulushiga pro-rata taqsimlanadi. Defolt "mudarib aybi" deb klassifikatsiya qilinsa, `MudaribShare` / mudarib kapitalidan qoplanadi | Reject (noto'g'ri taqsimot) |
| I11 | Ledger lotlari yig'indisi = bank hisobi qoldig'i (kunlik reconciliation) | Incident + alert |
| I12 | Append-only: UPDATE/DELETE yo'q, tuzatish faqat compensating tx bilan, hash chain butun | Incident |
| I13 | Kafolatlangan daromad va'dasi config'da ham, UI'da ham mavjud emas | Build/config validation |

---

## 7. Taqsimot (allocation)

- **Default model — segregated.** Investor aniq shartnomalardagi ulushga egalik qiladi (`Position`).
- Bo'sh kapital yangi shartnomalarga avtomatik taqsimlanadi, investorlarning bo'sh kapitaliga mutanosib. Shunda risk diversifikatsiya qilinadi va adolatli bo'ladi.
- Shartnoma ichida har bir to'lov va zarar pozitsiya ulushiga **pro-rata** taqsimlanadi.
- **FIFO** faqat bitta egasining lotlari ichida ishlatiladi: qaysi lot avval sarflangani tracing uchun kerak. Bu adolatga ta'sir qilmaydi, chunki egasi bir xil.
- **Pooled variant (opsional):** fund unit / NAV asosida, agar Sharia board shunday strukturani tasdiqlasa. Bunda lineage klass darajasida invariantlar uchun ishlaydi.

---

## 8. Arxitektura

```
 Operator UI/API ─▶ TxProposal
                       │
                       ▼
              ┌─────────────────┐     ┌──────────────────┐
              │  Policy Engine  │◀────│  Policy Store     │ (Sharia board imzolagan versiyalar)
              │  I1..I13        │     └──────────────────┘
              └────────┬────────┘
                 ok    │   reject → RejectedLog + alert
                       ▼
              ┌─────────────────┐
              │ Approval Service│  (I8, I9: Sharia board imzosi)
              └────────┬────────┘
                       ▼
              ┌─────────────────┐     ┌──────────────────┐
              │  Ledger Core    │────▶│ Hash chain +      │
              │  lots, lineage, │     │ kunlik Merkle root│──▶ Sharia board, regulyator
              │  append-only    │     └──────────────────┘
              └────────┬────────┘
                       ▼
              ┌─────────────────┐
              │ Bank Gateway    │  HSM imzo + outbox
              └────────┬────────┘
                       ▼
                 Hamkor bank (nominal hisob, 2-of-2 mandat)
                       │ statement / webhook
                       ▼
              Reconciliation (I11)

 Read side: Investor · Auditor · Regulyator · Operator dashboard'lari
 Yon modullar: Contract Module (murabaha, jadval, statuslar), Allocation Engine, Tracing & Freeze
```

### Komponentlar

- **Ledger Core** — lot, lineage va tx'lar; append-only event store.
- **Policy Engine** — har bir `TxProposal` ni barcha invariantlarga tekshiradi. Policy versiyalangan va Sharia board imzosi bilan faollashadi.
- **Approval Service** — rol asosidagi multi-sig (operator, Sharia board).
- **Contract Module** — murabaha shartnomalari, to'lov jadvali, statuslar (`Draft → GoodsPurchased → Active → Repaid / Overdue / Defaulted`), egalik hujjatlari.
- **Allocation Engine** — bo'sh kapitalni pozitsiyalarga taqsimlash, pro-rata hisob-kitob.
- **Bank Gateway** — payment order'larni imzolash, outbox, kiruvchi to'lovlarni lot'ga aylantirish.
- **Reconciliation** — kunlik ledger ↔ bank qoldig'i.
- **Tracing & Freeze** — FIFO forward/backward trace, freeze policy.
- **Dashboards** — investor, auditor, regulyator, operator.

---

## 9. Ma'lumotlar modeli (C#)

Lotlar **immutable**. Sarflash UTXO uslubida ishlaydi: parent `Spent` deb belgilanadi va undan child lotlar yaratiladi — biri oluvchiga, qoldiq (change) esa egasiga yangi lot bo'lib qaytadi. Shu sabab ledger to'liq append-only bo'ladi.

```csharp
public enum MoneyClass { Capital, Profit, InvestorProfit, MudaribShare, Reserve, Charity }

public enum TxKind
{
    Deposit, Deploy, Installment, ProfitSplit, Dividend, Withdrawal,
    Expense, RelatedParty, ReserveUse, LossAllocation, Donation, Compensation
}

public enum LotStatus { Active, Spent, Frozen }

public sealed record Lot(
    Guid Id,
    Guid AccountId,
    Guid? Owner,          // investor; fond darajasidagi klasslarda null
    MoneyClass Class,
    decimal Amount,
    long Seq,             // FIFO tartibi (bitta Owner ichida)
    Guid TxId,            // qaysi tx'da paydo bo'lgan
    Guid? ParentId,       // lineage
    Guid? ContractId);    // Profit / asosiy qism qaysi shartnomadan

public enum ContractStatus { Draft, GoodsPurchased, Active, Repaid, Overdue, Defaulted }

public sealed record Contract(
    Guid Id, Guid MerchantId, Guid CustomerId,
    decimal Cost, decimal Markup,
    ContractStatus Status,
    Guid? OwnershipDocId);  // I4 uchun

public sealed record Position(Guid ContractId, Guid InvestorId, decimal Principal, decimal Share);

public sealed record TxProposal(
    Guid Id, TxKind Kind, Guid? Beneficiary, decimal Amount,
    IReadOnlyList<Guid> InputLots, string Purpose, Guid ProposedBy);

public sealed record Approval(Guid TxId, string Role, string Signer, byte[] Signature, DateTimeOffset At);

public sealed record LedgerEntry(
    long Seq, Guid TxId, TxKind Kind, string PayloadJson,
    byte[] PrevHash, byte[] Hash, DateTimeOffset At);

public interface IPolicyRule
{
    string Id { get; }                                   // "I1", "I3", ...
    PolicyResult Check(TxProposal tx, ILedgerSnapshot state);
}

public abstract record PolicyResult
{
    public sealed record Ok : PolicyResult;
    public sealed record Reject(string RuleId, string Reason) : PolicyResult;
    public sealed record NeedsApproval(string RuleId, string Role) : PolicyResult;
}
```

---

## 10. Bank integratsiyasi (enforcement nuqtasi)

**Mandat:** nominal hisobdan chiquvchi har bir to'lov uchun operator imzosi **va** Omonat imzosi (HSM) kerak. Operator yolg'iz to'lov qila olmaydi. Butun g'oyaning kuchi shu nuqtada.

**To'lov oqimi:**
1. Operator `TxProposal` yuboradi.
2. Policy Engine barcha invariantlarni tekshiradi.
3. Kerak bo'lsa, Approval Service Sharia board imzosini kutadi.
4. Ledger'ga `Pending` entry yoziladi. Bank Gateway HSM bilan imzolangan payment order'ni outbox orqali yuboradi.
5. Bank ikkala imzoni tekshirib, to'lovni bajaradi.
6. Bank tasdiqlasa, entry `Committed` bo'ladi. Rad etsa, `Compensation` tx yoziladi.

**Kiruvchi to'lovlar:** bank statement yoki webhook orqali keladi va `Deposit` / `Installment` lotlariga aylanadi.

**Reconciliation:** har kuni ledger yig'indisi bank qoldig'i bilan solishtiriladi (I11). Farq chiqsa, incident ochiladi.

**Fallback:** bank 2-of-2 mandatni bermasa, tizim detection rejimida ishlaydi: tekshiruv va alert bor, lekin to'lovni to'xtatib bo'lmaydi. Bu kuchsizroq rejim va mijozga ochiq aytiladi.

---

## 11. Ishonch va audit

- **Append-only:** DB darajasida UPDATE/DELETE huquqlari olib tashlangan. Tuzatish faqat compensating tx bilan qilinadi.
- **Hash chain:** har bir entry oldingi entry'ning hashini o'z ichiga oladi.
- **Anchoring:** har kuni Merkle root Sharia board va regulyatorga yuboriladi. Operator tarixni qayta yozsa, root mos kelmay qoladi.
- **Rejected log:** rad etilgan urinishlar ham o'chirilmaydi.
- **Separation of duties:** operator, Sharia board, tizim administratori — alohida rollar va alohida kalitlar.

---

## 12. Ko'rinishlar

**Investor**
- Jami kiritilgan / likvid / faol / qaytgan asosiy qism.
- Olingan foyda / kutilayotgan foyda ("kafolatlanmagan" belgisi bilan).
- Muammoli (overdue) / defolt va zarar / muzlatilgan summa.
- Pozitsiyalar ro'yxati: har bir shartnoma statusi bilan.
- Har bir dividend'ning lineage'i: shartnoma, oy, ustama.
- Yechib olish: hozir qancha mumkin va qolgani qachon bo'shashi.

**Sharia auditor**
- Real-time ledger, rejected log, approval navbati.
- Policy versiyalari va ularning imzolari.
- Rezerv harakati, related-party o'tkazmalar, charity oqimi.
- Kunlik Merkle root tekshiruvi.

**Regulyator**
- Agregat ko'rsatkichlar: AUM, NPL ulushi, rezerv, likvidlik.
- Incident'lar va reconciliation natijalari.

**Operator**
- Tx taklif qilish, approval statusi, rad sabablari.
- Shartnomalar va to'lov jadvali.

---

## 13. Tracing va freeze

- Kiruvchi pul firibgarlik bilan bog'liq chiqsa (masalan, o'g'irlangan kartadan qilingan depozit), FIFO bo'yicha forward va backward trace qilinadi.
- **Freeze ≠ reversal.** Muzlatish vaqtinchalik, qaytarish faqat tekshiruvdan keyin va faqat compensating tx bilan.
- Freeze policy: hop limit, vaqt oynasi, vijdonli (good-faith) oluvchini himoya qilish qoidalari.

---

## 14. Huquqiy kontekst

- Islom banking qonuni qabul qilingan va bir nechta bank islomiy faoliyat litsenziyasini olgan. Bu asosiy mijoz segmenti.
- 2026-yil 14-avgust farmoniga ko'ra, 2027-yil 1-yanvardan muddatli to'lov operatorlari Markaziy bank reyestriga kiritiladi va jismoniy shaxslardan mablag' jalb qilishi taqiqlanadi. Shuning uchun target holatda mudarib litsenziyali subyekt (bank yoki fond) deb olingan. Farmonning aniq qo'llanilishi yuristlar bilan tekshiriladi.
- Markaziy bank raqamli so'm (ulgurji, 2026–2029 tadqiqot) ustida ishlayapti. Kelajakda shu qoidalar raqamli so'm ustida programmable money sifatida ishlashi mumkin, lekin hozir buni kutish shart emas.

---

## 15. Mijozlar va monetizatsiya

| Segment | Nima oladi |
|---|---|
| Islomiy bank oynalari | Mudaraba investitsiya hisoblari uchun enforcement ledger va investor dashboard |
| Litsenziyali investitsiya fondlari va platformalar | Shaffoflikni texnik isbotlash — investor ishonchi |
| Sharia audit firmalari | Real-time audit vositasi, auditor seat'lari |

Narx modeli: AUM'dan bps + auditor va regulyator seat'lari.

---

## 16. Texnologik stek

- **.NET 10 (LTS), ASP.NET Core** — API, Policy Engine, worker'lar.
- **PostgreSQL** — event store (append-only, UPDATE/DELETE revoke), read model'lar alohida.
- **Blazor** — investor, auditor, regulyator va operator dashboard'lari.
- **Outbox pattern** — bank integratsiyasi.
- **HSM / KMS** — Omonat imzo kaliti va Merkle root imzosi.
- **OpenTelemetry** — trace va audit.
- Ledger write path'da serializable tranzaksiyalar, read side'da proyeksiyalar.

---

## 17. Ish hajmi (bitta yaxlit scope)

### Yetkazib beriladigan narsalar
1. Ledger Core: lot va lineage, append-only event store, hash chain, kunlik Merkle root.
2. Policy Engine: I1–I13 invariantlari, versiyalangan policy, Sharia board imzosi bilan faollashtirish.
3. Approval Service: multi-sig (operator + Sharia board).
4. Contract Module: murabaha shartnomasi, to'lov jadvali, statuslar, egalik hujjatlari.
5. Allocation Engine: segregated model, pro-rata taqsimot, defolt va zarar taqsimoti.
6. Bank Gateway: HSM imzo, outbox, kiruvchi to'lovlarni qabul qilish, reconciliation.
7. Tracing & Freeze moduli.
8. To'rtta dashboard: investor, auditor, regulyator, operator.
9. Rejected log va alerting.
10. Hujjatlar: policy spec (Sharia board uchun), bank integratsiya spec, operator qo'llanmasi.

### Qabul mezonlari (har bir invariant test bilan isbotlanadi)

| Test | Kutilgan natija |
|---|---|
| T1 | Yangi investor kapitalidan dividend → rad (I1) |
| T2 | Investor A yechishi B lotlaridan → rad (I2) |
| T3 | Marketing to'lovi `Capital` dan → rad (I3) |
| T4 | Tovar sotib olinmagan shartnomani faollashtirish → rad (I4) |
| T5 | Rezerv cap + 1 so'm → rad (I5) |
| T6 | Kechikish jarimasini `Profit` ga yozish → rad (I7) |
| T7 | Related-party to'lovi Sharia board imzosisiz → bajarilmaydi (I8) |
| T8 | Defolt → zarar pozitsiya egalariga pro-rata, yig'indi aniq mos (I10) |
| T9 | DB'da qo'lda UPDATE → hash chain buzilishi aniqlanadi (I12) |
| T10 | Ledger ≠ bank qoldig'i → incident (I11) |
| T11 | Har bir dividend lineage'i shartnoma ustamasigacha yetib boradi |
| T12 | Operator Omonat imzosisiz bankka to'lov yubora olmaydi |
| T13 | Likvidi yetmagan investorga faqat likvid qism beriladi, qolgani jadval bilan ko'rsatiladi |

---

## 18. Ochiq savollar (qaror kerak)

- Huquqiy forma va hamkor bank: kim mudarib, kim nominal hisobni ochadi, 2-of-2 mandat mumkinmi.
- Sharia board tasdig'i kerak bo'lgan masalalar: foyda taqsimoti tartibi, rezerv turlari (AAOIFI'dagi PER / IRR), direct deal cost'larni kapitaldan qilishga ruxsat.
- Allocation policy parametrlari: shartnomaga minimal ulush, konsentratsiya limiti.
- Defolt klassifikatsiyasi: "mudarib aybi" mezonlari kim tomonidan va qanday belgilanadi.
- KYC/AML integratsiyasi.
