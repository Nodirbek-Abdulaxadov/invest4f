// Omonat API turlari va mijoz. Barcha summalar tiyinda (1 so'm = 100 tiyin).

import { useCallback, useEffect, useRef, useState } from 'react'

export type Money = number
export type MoneyClass = 'Capital' | 'Profit' | 'InvestorProfit' | 'MudaribShare' | 'Reserve' | 'Charity'
export type TxKind =
  | 'Deposit' | 'Deploy' | 'Installment' | 'ProfitSplit' | 'Dividend' | 'Withdrawal'
  | 'Expense' | 'ReserveUse' | 'LossAllocation' | 'Donation' | 'Compensation'
export type ContractStatus = 'Draft' | 'GoodsPurchased' | 'Active' | 'Repaid' | 'Overdue' | 'Defaulted'
export type PartyKind = 'Investor' | 'Merchant' | 'Customer' | 'Vendor' | 'Charity'
export type PaymentStatus = 'Pending' | 'Confirmed' | 'Failed'
export type LotStatus = 'Active' | 'Spent' | 'Frozen'
export type Fault = 'None' | 'Mudarib'

export interface Party { id: string; kind: PartyKind; name: string; bank_account: string }
export type Actor =
  | { role: 'Operator' | 'ShariaBoard' | 'Regulator' | 'Admin' | 'Bank' | 'System' }
  | { role: 'Investor'; id: string }

export interface Violation { rule_id: string; reason: string }
export interface NeedsApproval { rule_id: string; role: string; reason: string }
export type Outcome =
  | { status: 'Executed'; tx_id: string; payments: string[] }
  | { status: 'PendingApproval'; proposal_id: string; needs: NeedsApproval[] }
  | { status: 'Rejected'; violations: Violation[] }
  | { status: 'Recorded' }

export type Source = { type: 'Lot'; lot_id: string } | { type: 'Inflow' } | { type: 'Refund'; lot_id: string }
export type Dest = { type: 'Lot'; class: MoneyClass; owner: string | null } | { type: 'External'; party: string }
export interface Leg { source: Source; dest: Dest; amount: Money }
export interface Share { investor: string; amount: Money }
export interface TxProposal {
  id: string
  kind: TxKind
  contract_id?: string | null
  legs: Leg[]
  write_offs?: Share[]
  fault?: Fault | null
  bank_ref?: string | null
  refund_of?: string | null
  purpose: string
  proposed_by: Actor
}

export interface Lot {
  id: string; owner: string | null; class: MoneyClass; amount: Money; seq: number
  tx_id: string; parent_id: string | null; contract_id: string | null; created_at: string
}
export interface Installment { no: number; due: string; principal: Money; markup: Money }
export interface OwnershipDoc { doc_no: string; description: string; sha256: string; attached_at: string }
export interface Contract {
  id: string; merchant: string; customer: string; goods: string; cost: Money; markup: Money
  schedule: Installment[]; status: ContractStatus; ownership_doc: OwnershipDoc | null
  purchase_tx: string | null; paid_principal: Money; paid_markup: Money; late_fees: Money
  fault: Fault | null; mudarib_liability: Money; created_at: string
}
export interface Position {
  contract_id: string; investor: string; principal: Money; repaid: Money; written_off: Money; covered: Money
}
export interface ContractView {
  contract: Contract; effective_status: ContractStatus; due_until_today: Money; positions: Position[]
  profit_undistributed: Money; merchant_name: string; customer_name: string
}
export interface Payment {
  id: string; tx_id: string; kind: TxKind; beneficiary: string; amount: Money
  sources: [string, Money][]; purpose: string; created_at: string
}
export interface PaymentView { payment: Payment; status: PaymentStatus; bank_note: string | null; beneficiary_name: string }
export interface Approval { subject: string; role: string; signer: string; signature: string; public_key: string; at: string }
export interface AppliedTx {
  proposal: TxProposal; policy_version: number | null; approvals: Approval[]; spent: string[]
  created: Lot[]; payments: Payment[]; at: string
}
export interface TxView { tx: AppliedTx; payments: PaymentView[] }
export type Attempt =
  | { type: 'Tx'; proposal: TxProposal }
  | { type: 'ActivateContract'; contract_id: string; by: Actor }
  | { type: 'Policy'; version: number; by: Actor }
  | { type: 'Other'; action: string; by: Actor }
export interface RejectedAttempt { id: string; attempt: Attempt; violations: Violation[]; at: string }
export interface PendingApproval {
  proposal: TxProposal; needs: NeedsApproval[]; approvals: Approval[]; policy_version: number | null; created_at: string
}
export interface ResolvedApproval { pending: PendingApproval; outcome: 'Executed' | 'Denied' | 'Rejected'; note: string; at: string }
export interface Policy {
  version: number; reserve_cap_bps: number; mudarib_share_bps: number
  related_parties: string[]; approved_charities: string[]; product_description: string
}
export interface PolicyRecord { policy: Policy; proposed_by: Actor; proposed_at: string; activation: Approval | null }
export interface Anchor {
  day: string; from_seq: number; to_seq: number; root: string; signature: string; public_key: string; at: string
}
export interface Incident { id: string; kind: string; details: string; at: string }
export interface Reconciliation { id: string; ledger_balance: Money; bank_balance: Money; ok: boolean; at: string }
export interface FreezeCase {
  id: string; root_lot: string; lots: string[]; flagged: string[]; reason: string; hop_limit: number
  window_days: number; opened_by: Actor; opened_at: string; closed_at: string | null
}
export interface LineageStep {
  lot_id: string; class: MoneyClass; owner: string | null; amount: Money; contract_id: string | null
  tx_id: string; tx_kind: TxKind | null; bank_ref: string | null; at: string
}
export interface ClassBalance { class: MoneyClass; amount: Money }
export interface Movement { tx_id: string; kind: TxKind; amount: Money; direction: 'in' | 'out'; purpose: string; at: string }

export interface PositionView {
  contract_id: string; goods: string; status: ContractStatus; principal: Money; repaid: Money; outstanding: Money
  written_off: Money; covered: Money; share_bps: number; expected_profit: Money; expected_profit_label: string
}
export interface DividendSource {
  amount: Money; contract_id: string | null; goods: string | null; month: string | null
  installment_tx: string | null; lineage: LineageStep[]
}
export interface DividendView { payment_id: string; amount: Money; status: PaymentStatus; at: string; sources: DividendSource[] }
export interface InvestorDashboard {
  investor: Party; deposited: Money; liquid: Money; active: Money; returned_principal: Money; withdrawn: Money
  profit_credited: Money; profit_available: Money; dividends_paid: Money; expected_profit: Money
  expected_profit_label: string; overdue: Money; overdue_contracts: number; defaulted_loss: Money
  covered_loss: Money; frozen: Money; positions: PositionView[]; dividends: DividendView[]; withdrawable_now: Money
}
export interface AuditorView {
  balances: ClassBalance[]; expected_bank_balance: Money; pending: PendingApproval[]; approval_history: ResolvedApproval[]
  rejected: RejectedAttempt[]; reserve_movements: Movement[]; charity_movements: Movement[]
  related_party_payments: PaymentView[]; policies: PolicyRecord[]; active_policy: number | null; anchors: Anchor[]
  incidents: Incident[]; freezes: FreezeCase[]; head_seq: number
}
export interface RegulatorView {
  aum: Money; liquidity: Money; liquidity_bps: number; outstanding: Money; npl: Money; npl_bps: number
  written_off: Money; reserve: Money; charity: Money; investors: number
  contracts_by_status: Record<string, number>; incidents: Incident[]; reconciliations: Reconciliation[]
  rejected_count: number; anchors: Anchor[]
}
export interface LedgerEntry { seq: number; at: string; kind: string; payload: string; prev_hash: string; hash: string }
export interface AnchorCheck { anchor: Anchor; recomputed_root: string; ok: boolean }
export interface VerifyReport {
  entries: number; chain_ok: boolean; chain_error: string | null; anchors: AnchorCheck[]; store_matches_memory: boolean
}
export interface Release { date: string; amount: Money; cumulative: Money }
export interface WithdrawalPlan {
  requested: Money; payable_now: Money; remaining: Money; schedule: Release[]; proposal: TxProposal | null
}
export interface Health {
  ok: boolean; integrity_error: string | null; store: string; head_seq: number; dev: boolean
  today: string; clock_offset_days: number
}
export interface Meta { parties: Party[]; policy: Policy | null; active_policy: number | null; dev: boolean; today: string }
export interface StatementLine {
  bank_ref: string; direction: 'In' | 'Out'; amount: Money; counterparty: string; memo: string
  payment_id: string | null; at: string
}
export interface OutboxItem { id: string; ledger_seq: number; body: string; status: string; attempts: number; note: string | null; created_at: string }

// ------------------------------------------------------------------ sessiya (dev autentifikatsiya)

export interface Session { actor: string; label: string }

const KEY = 'omonat.session'

export function getSession(): Session | null {
  const raw = localStorage.getItem(KEY)
  return raw ? (JSON.parse(raw) as Session) : null
}

export function setSession(s: Session | null) {
  if (s) localStorage.setItem(KEY, JSON.stringify(s))
  else localStorage.removeItem(KEY)
}

export function investorId(s: Session | null): string | null {
  return s?.actor.startsWith('investor:') ? s.actor.slice('investor:'.length) : null
}

export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function req<T>(method: string, path: string, body?: unknown, actor?: string): Promise<T> {
  const who = actor ?? getSession()?.actor
  const headers: Record<string, string> = {}
  if (who) headers['X-Omonat-Actor'] = who
  if (body !== undefined) headers['Content-Type'] = 'application/json'
  const res = await fetch(path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) })
  const text = await res.text()
  const data = text ? JSON.parse(text) : null
  if (!res.ok) throw new ApiError(res.status, data?.error ?? res.statusText)
  return data as T
}

export const api = {
  get: <T>(path: string, actor?: string) => req<T>('GET', path, undefined, actor),
  post: <T>(path: string, body: unknown = {}, actor?: string) => req<T>('POST', path, body, actor),
}

/// Ma'lumotni yuklaydi va `intervalMs` bo'yicha yangilaydi.
export function useApi<T>(path: string | null, intervalMs = 5000) {
  const [data, setData] = useState<T | null>(null)
  const [error, setError] = useState<string | null>(null)
  const alive = useRef(true)
  const load = useCallback(async () => {
    if (!path) return
    try {
      const d = await api.get<T>(path)
      if (alive.current) {
        setData(d)
        setError(null)
      }
    } catch (e) {
      if (alive.current) setError((e as Error).message)
    }
  }, [path])
  useEffect(() => {
    alive.current = true
    load()
    const t = intervalMs > 0 ? setInterval(load, intervalMs) : undefined
    return () => {
      alive.current = false
      if (t) clearInterval(t)
    }
  }, [load, intervalMs])
  return { data, error, reload: load }
}
