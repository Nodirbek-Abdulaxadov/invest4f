//! Policy Engine: har bir `TxProposal` barcha qoidalarga bajarilishidan oldin tekshiriladi.

pub mod config;
pub mod rules;

use crate::model::{Approval, NeedsApproval, Role, TxProposal, Violation};
use crate::policy::config::Policy;
use crate::state::State;
use chrono::{DateTime, Utc};

/// Qoida tekshiruvi uchun kontekst: ledger holati (snapshot), faol policy, mavjud imzolar.
pub struct RuleCtx<'a> {
    pub state: &'a State,
    pub policy: Option<&'a Policy>,
    pub now: DateTime<Utc>,
    pub approvals: &'a [Approval],
}

impl RuleCtx<'_> {
    pub fn has_approval(&self, role: Role) -> bool {
        self.approvals.iter().any(|a| a.role == role)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyResult {
    Ok,
    Reject { rule_id: String, reason: String },
    NeedsApproval { rule_id: String, role: Role, reason: String },
}

pub trait PolicyRule: Send + Sync {
    /// "I1", "I3", ...
    fn id(&self) -> &'static str;
    fn check(&self, tx: &TxProposal, ctx: &RuleCtx) -> PolicyResult;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Evaluation {
    pub violations: Vec<Violation>,
    pub needs: Vec<NeedsApproval>,
}

impl Evaluation {
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty() && self.needs.is_empty()
    }

    pub fn rejected_by(&self, rule_id: &str) -> bool {
        self.violations.iter().any(|v| v.rule_id == rule_id)
    }
}

/// Barcha qoidalarni ishga tushiradi. Strukturaviy xato bo'lsa, qolgan qoidalar tekshirilmaydi
/// (ular mavjud lot va shartnomaga tayanadi).
pub fn evaluate(rules: &[Box<dyn PolicyRule>], tx: &TxProposal, ctx: &RuleCtx) -> Evaluation {
    let mut ev = Evaluation::default();
    for rule in rules {
        match rule.check(tx, ctx) {
            PolicyResult::Ok => {}
            PolicyResult::Reject { rule_id, reason } => {
                let structural = rule_id == rules::STRUCT;
                ev.violations.push(Violation { rule_id, reason });
                if structural {
                    return ev;
                }
            }
            PolicyResult::NeedsApproval { rule_id, role, reason } => {
                ev.needs.push(NeedsApproval { rule_id, role, reason });
            }
        }
    }
    ev
}

pub fn default_rules() -> Vec<Box<dyn PolicyRule>> {
    use rules::*;
    vec![
        Box::new(Structural),
        Box::new(I1Dividend),
        Box::new(I2Segregation),
        Box::new(I3Expense),
        Box::new(I4Deploy),
        Box::new(I5Reserve),
        Box::new(I6MudaribShare),
        Box::new(I7LateFeeCharity),
        Box::new(I8RelatedParty),
        Box::new(I9ReserveUse),
        Box::new(I10ProRata),
        Box::new(Flow),
    ]
}
