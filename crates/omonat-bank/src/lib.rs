//! Bank integratsiyasi: imzo kaliti (HSM o'rnida dasturiy Ed25519), payment order va
//! 2-of-2 mandatni tekshiradigan mock bank.

pub mod mock;
pub mod order;
pub mod signer;

pub use mock::{BankError, MockBank, StatementLine};
pub use order::{sign_order, PaymentOrder, SignedOrder};
pub use signer::{verify, Signer, SoftwareSigner};
