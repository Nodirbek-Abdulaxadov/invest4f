//! Ledger hodisalari. Ledger — shu hodisalarning append-only zanjiri; holat ulardan quriladi.

use crate::model::*;
use crate::money::Money;
use crate::policy::config::Policy;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    PartyRegistered { party: Party, by: Actor },
    PolicyProposed { policy: Policy, by: Actor },
    PolicyActivated { version: u32, approval: Approval },
    ContractCreated { contract: Contract, by: Actor },
    OwnershipDocAttached { contract_id: ContractId, doc: OwnershipDoc, by: Actor },
    ContractActivated { contract_id: ContractId, by: Actor },
    TxApplied { tx: AppliedTx },
    TxAwaitingApproval { proposal: TxProposal, needs: Vec<NeedsApproval>, policy_version: Option<u32> },
    ApprovalGranted { proposal_id: TxId, approval: Approval },
    ApprovalDenied { proposal_id: TxId, by: Actor, reason: String },
    Rejected { attempt: RejectedAttempt },
    PaymentConfirmed { payment_id: PaymentId, bank_ref: String },
    PaymentFailed { payment_id: PaymentId, reason: String },
    LotsFrozen { case: FreezeCase },
    LotsUnfrozen { case_id: CaseId, by: Actor, reason: String },
    Reconciled { reconciliation: Reconciliation },
    IncidentOpened { incident: Incident },
    MerkleAnchored { anchor: Anchor },
}

impl Event {
    pub fn kind(&self) -> &'static str {
        match self {
            Event::PartyRegistered { .. } => "PartyRegistered",
            Event::PolicyProposed { .. } => "PolicyProposed",
            Event::PolicyActivated { .. } => "PolicyActivated",
            Event::ContractCreated { .. } => "ContractCreated",
            Event::OwnershipDocAttached { .. } => "OwnershipDocAttached",
            Event::ContractActivated { .. } => "ContractActivated",
            Event::TxApplied { .. } => "TxApplied",
            Event::TxAwaitingApproval { .. } => "TxAwaitingApproval",
            Event::ApprovalGranted { .. } => "ApprovalGranted",
            Event::ApprovalDenied { .. } => "ApprovalDenied",
            Event::Rejected { .. } => "Rejected",
            Event::PaymentConfirmed { .. } => "PaymentConfirmed",
            Event::PaymentFailed { .. } => "PaymentFailed",
            Event::LotsFrozen { .. } => "LotsFrozen",
            Event::LotsUnfrozen { .. } => "LotsUnfrozen",
            Event::Reconciled { .. } => "Reconciled",
            Event::IncidentOpened { .. } => "IncidentOpened",
            Event::MerkleAnchored { .. } => "MerkleAnchored",
        }
    }

    /// Ledger zanjiridagi pul miqdori (ko'rinish uchun).
    pub fn amount(&self) -> Option<Money> {
        match self {
            Event::TxApplied { tx } => Some(tx.proposal.total()),
            _ => None,
        }
    }
}
