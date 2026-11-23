//! Domen modeli: ishtirokchilar, lotlar, shartnomalar, pozitsiyalar, tx takliflari.

use crate::money::Money;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type PartyId = Uuid;
pub type LotId = Uuid;
pub type TxId = Uuid;
pub type ContractId = Uuid;
pub type PaymentId = Uuid;
pub type CaseId = Uuid;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MoneyClass {
    Capital,
    Profit,
    InvestorProfit,
    MudaribShare,
    Reserve,
    Charity,
}

impl MoneyClass {
    /// Egasi investor bo'lgan klasslar. Qolganlari fond darajasida (Owner = None).
    pub fn is_owner_bound(self) -> bool {
        matches!(self, MoneyClass::Capital | MoneyClass::InvestorProfit)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TxKind {
    Deposit,
    Deploy,
    Installment,
    ProfitSplit,
    Dividend,
    Withdrawal,
    Expense,
    ReserveUse,
    LossAllocation,
    Donation,
    Compensation,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LotStatus {
    Active,
    Spent,
    Frozen,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PartyKind {
    Investor,
    Merchant,
    Customer,
    Vendor,
    Charity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Party {
    pub id: PartyId,
    pub kind: PartyKind,
    pub name: String,
    pub bank_account: String,
}

/// Rollar. Har bir rol alohida kalit bilan ishlaydi (separation of duties).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    Operator,
    ShariaBoard,
    Investor,
    Regulator,
    Admin,
    Bank,
    System,
}

/// Amalni bajaruvchi. `Investor` faqat o'z nomidan ish ko'radi.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", content = "id")]
pub enum Actor {
    Operator,
    ShariaBoard,
    Investor(PartyId),
    Regulator,
    Admin,
    Bank,
    System,
}

impl Actor {
    pub fn role(&self) -> Role {
        match self {
            Actor::Operator => Role::Operator,
            Actor::ShariaBoard => Role::ShariaBoard,
            Actor::Investor(_) => Role::Investor,
            Actor::Regulator => Role::Regulator,
            Actor::Admin => Role::Admin,
            Actor::Bank => Role::Bank,
            Actor::System => Role::System,
        }
    }
}

/// Pul bo'lagi. Yaratilgandan keyin o'zgarmaydi; holati (`LotStatus`) hodisalardan kelib chiqadi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lot {
    pub id: LotId,
    /// Investor; fond darajasidagi klasslarda `None`.
    pub owner: Option<PartyId>,
    pub class: MoneyClass,
    pub amount: Money,
    /// FIFO tartibi (bitta Owner ichida). Global monoton.
    pub seq: u64,
    /// Qaysi tx'da paydo bo'lgan.
    pub tx_id: TxId,
    /// Lineage: qaysi lotdan ajralgan. Bankdan kirgan pulda `None`.
    pub parent_id: Option<LotId>,
    /// Profit / asosiy qism qaysi shartnomadan.
    pub contract_id: Option<ContractId>,
    pub created_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractStatus {
    Draft,
    GoodsPurchased,
    Active,
    Repaid,
    Overdue,
    Defaulted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installment {
    pub no: u32,
    pub due: NaiveDate,
    pub principal: Money,
    pub markup: Money,
}

/// Tovarga egalik hujjati (qabz, yuk xati). I4 uchun.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnershipDoc {
    pub doc_no: String,
    pub description: String,
    /// Hujjat faylining SHA-256 xeshi (hex).
    pub sha256: String,
    pub attached_at: DateTime<Utc>,
}

/// Defolt sababi: mijoz aybi yoki mudarib aybi (ta'addi / taqsir).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fault {
    None,
    Mudarib,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub id: ContractId,
    pub merchant: PartyId,
    pub customer: PartyId,
    pub goods: String,
    pub cost: Money,
    pub markup: Money,
    pub schedule: Vec<Installment>,
    pub status: ContractStatus,
    pub ownership_doc: Option<OwnershipDoc>,
    /// Tovar xaridi (Deploy) tx'i.
    pub purchase_tx: Option<TxId>,
    pub paid_principal: Money,
    pub paid_markup: Money,
    pub late_fees: Money,
    pub fault: Option<Fault>,
    /// Mudarib aybi bilan defoltda MudaribShare yetmagan qism.
    pub mudarib_liability: Money,
    pub created_at: DateTime<Utc>,
}

impl Contract {
    pub fn price(&self) -> Money {
        self.cost + self.markup
    }

    pub fn paid_total(&self) -> Money {
        self.paid_principal + self.paid_markup
    }

    pub fn remaining_principal(&self) -> Money {
        self.cost - self.paid_principal
    }

    pub fn remaining_markup(&self) -> Money {
        self.markup - self.paid_markup
    }

    /// `on` sanasigacha to'lanishi kerak bo'lgan jami (asosiy + ustama).
    pub fn due_until(&self, on: NaiveDate) -> Money {
        self.schedule
            .iter()
            .filter(|i| i.due <= on)
            .map(|i| i.principal + i.markup)
            .sum()
    }

    /// Saqlangan status + sana bo'yicha kechikish.
    pub fn effective_status(&self, today: NaiveDate) -> ContractStatus {
        if self.status == ContractStatus::Active && self.paid_total() < self.due_until(today) {
            ContractStatus::Overdue
        } else {
            self.status
        }
    }
}

/// Investorning aniq shartnomadagi ulushi (segregated model).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub contract_id: ContractId,
    pub investor: PartyId,
    pub principal: Money,
    pub repaid: Money,
    pub written_off: Money,
    /// Rezerv yoki mudarib hisobidan qoplangan zarar.
    pub covered: Money,
}

impl Position {
    pub fn outstanding(&self) -> Money {
        self.principal - self.repaid - self.written_off
    }

    pub fn uncovered_loss(&self) -> Money {
        self.written_off - self.covered
    }
}

/// Leg manbasi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Source {
    /// Mavjud lotdan sarflash.
    Lot { lot_id: LotId },
    /// Bankka kirgan pul (bank_ref proposal'da).
    Inflow,
    /// Bank rad etgan to'lovning qaytgan puli; `lot_id` — asl manba lot.
    Refund { lot_id: LotId },
}

/// Leg manzili.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Dest {
    /// Ledger ichida yangi lot.
    Lot { class: MoneyClass, owner: Option<PartyId> },
    /// Bank orqali tashqi oluvchiga.
    External { party: PartyId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leg {
    pub source: Source,
    pub dest: Dest,
    pub amount: Money,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    pub investor: PartyId,
    pub amount: Money,
}

/// Operator (yoki bank kanali) taklif qilgan tranzaksiya. Bajarilishidan oldin policy tekshiradi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxProposal {
    pub id: TxId,
    pub kind: TxKind,
    #[serde(default)]
    pub contract_id: Option<ContractId>,
    pub legs: Vec<Leg>,
    /// Faqat `LossAllocation`: har bir investorning hisobdan chiqariladigan zarari.
    #[serde(default)]
    pub write_offs: Vec<Share>,
    /// Faqat `LossAllocation`.
    #[serde(default)]
    pub fault: Option<Fault>,
    /// Kiruvchi pul yoki kompensatsiya uchun bank havolasi.
    #[serde(default)]
    pub bank_ref: Option<String>,
    /// Faqat `Compensation`: qaysi to'lov qaytdi.
    #[serde(default)]
    pub refund_of: Option<PaymentId>,
    pub purpose: String,
    pub proposed_by: Actor,
}

impl TxProposal {
    pub fn new(kind: TxKind, proposed_by: Actor, purpose: impl Into<String>) -> Self {
        TxProposal {
            id: Uuid::new_v4(),
            kind,
            contract_id: None,
            legs: Vec::new(),
            write_offs: Vec::new(),
            fault: None,
            bank_ref: None,
            refund_of: None,
            purpose: purpose.into(),
            proposed_by,
        }
    }

    pub fn total(&self) -> Money {
        self.legs.iter().map(|l| l.amount).sum()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub subject: Uuid,
    pub role: Role,
    pub signer: String,
    /// Ed25519 imzo (hex), subject hash ustidan.
    pub signature: String,
    pub public_key: String,
    pub at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentStatus {
    Pending,
    Confirmed,
    Failed,
}

/// Tashqi oluvchiga chiquvchi to'lov (bank orqali).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub id: PaymentId,
    pub tx_id: TxId,
    pub kind: TxKind,
    pub beneficiary: PartyId,
    pub amount: Money,
    /// Qaysi lotlardan qancha (lineage).
    pub sources: Vec<(LotId, Money)>,
    pub purpose: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PositionDelta {
    Open { contract_id: ContractId, investor: PartyId, amount: Money },
    Cancel { contract_id: ContractId, investor: PartyId, amount: Money },
    Repay { contract_id: ContractId, investor: PartyId, amount: Money },
    WriteOff { contract_id: ContractId, investor: PartyId, amount: Money },
    Cover { contract_id: ContractId, investor: PartyId, amount: Money },
}

/// Policy'dan o'tgan va ledger'ga yozilgan tx: kiruvchi taklif + undan aniqlangan barcha ta'sirlar.
/// Ta'sirlar shu yerda saqlanadi, shuning uchun replay deterministik.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedTx {
    pub proposal: TxProposal,
    pub policy_version: Option<u32>,
    pub approvals: Vec<Approval>,
    pub spent: Vec<LotId>,
    pub created: Vec<Lot>,
    pub payments: Vec<Payment>,
    pub positions: Vec<PositionDelta>,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    pub rule_id: String,
    pub reason: String,
}

/// Rad etilgan urinish nimaga tegishli edi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Attempt {
    Tx { proposal: TxProposal },
    ActivateContract { contract_id: ContractId, by: Actor },
    Policy { version: u32, by: Actor },
    Other { action: String, by: Actor },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedAttempt {
    pub id: Uuid,
    pub attempt: Attempt,
    pub violations: Vec<Violation>,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsApproval {
    pub rule_id: String,
    pub role: Role,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreezeCase {
    pub id: CaseId,
    pub root_lot: LotId,
    pub lots: Vec<LotId>,
    /// Trace qilingan, lekin vijdonli oluvchi sifatida muzlatilmagan lotlar.
    pub flagged: Vec<LotId>,
    pub reason: String,
    pub hop_limit: u32,
    pub window_days: u32,
    pub opened_by: Actor,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentKind {
    ReconciliationMismatch,
    HashChainBroken,
    AnchorMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub id: Uuid,
    pub kind: IncidentKind,
    pub details: String,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciliation {
    pub id: Uuid,
    /// Ledger bo'yicha bankda bo'lishi kerak bo'lgan qoldiq.
    pub ledger_balance: Money,
    pub bank_balance: Money,
    pub ok: bool,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub day: NaiveDate,
    pub from_seq: u64,
    pub to_seq: u64,
    /// Merkle root (hex).
    pub root: String,
    /// Omonat kaliti bilan root imzosi (hex).
    pub signature: String,
    pub public_key: String,
    pub at: DateTime<Utc>,
}
