import { useState, type ReactNode } from 'react'
import type { ContractStatus, Outcome, PaymentStatus, Violation } from '../api'
import { paymentLabel, ruleLabel, som, statusLabel } from '../format'

export function Card({ title, actions, children }: { title?: ReactNode; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="card">
      {(title || actions) && (
        <header className="card-head">
          <h2>{title}</h2>
          <div className="card-actions">{actions}</div>
        </header>
      )}
      {children}
    </section>
  )
}

export function Tiles({ children }: { children: ReactNode }) {
  return <div className="tiles">{children}</div>
}

export function Tile({ label, value, note, tone }: { label: string; value: ReactNode; note?: ReactNode; tone?: 'good' | 'warn' | 'bad' | 'muted' }) {
  return (
    <div className={`tile ${tone ?? ''}`}>
      <div className="tile-label">{label}</div>
      <div className="tile-value">{value}</div>
      {note && <div className="tile-note">{note}</div>}
    </div>
  )
}

export function Money({ v }: { v: number | null | undefined }) {
  return <span className="money">{som(v)}</span>
}

export function Badge({ tone = 'muted', children, title }: { tone?: 'good' | 'warn' | 'bad' | 'muted' | 'info'; children: ReactNode; title?: string }) {
  return (
    <span className={`badge ${tone}`} title={title}>
      {children}
    </span>
  )
}

const statusTone: Record<ContractStatus, 'good' | 'warn' | 'bad' | 'muted' | 'info'> = {
  Draft: 'muted',
  GoodsPurchased: 'info',
  Active: 'good',
  Repaid: 'good',
  Overdue: 'warn',
  Defaulted: 'bad',
}

export function StatusBadge({ s }: { s: ContractStatus }) {
  return <Badge tone={statusTone[s]}>{statusLabel[s]}</Badge>
}

export function PaymentBadge({ s }: { s: PaymentStatus }) {
  return <Badge tone={s === 'Confirmed' ? 'good' : s === 'Failed' ? 'bad' : 'info'}>{paymentLabel[s]}</Badge>
}

export function RuleBadge({ id }: { id: string }) {
  return (
    <Badge tone="bad" title={ruleLabel[id]}>
      {id}
    </Badge>
  )
}

export function Violations({ list }: { list: Violation[] }) {
  return (
    <ul className="violations">
      {list.map((v, i) => (
        <li key={i}>
          <RuleBadge id={v.rule_id} /> <span>{v.reason}</span>
        </li>
      ))}
    </ul>
  )
}

export function OutcomeView({ o }: { o: Outcome | null }) {
  if (!o) return null
  switch (o.status) {
    case 'Executed':
      return (
        <div className="outcome good">
          ✔ Policy'dan o'tdi va ledger'ga yozildi{o.payments.length > 0 && <> — {o.payments.length} ta to'lov bankka yuborildi</>}.
        </div>
      )
    case 'Recorded':
      return <div className="outcome good">✔ Ledger'ga yozildi.</div>
    case 'PendingApproval':
      return (
        <div className="outcome warn">
          ⏸ Bajarilmadi — imzo kutilmoqda:
          <ul>
            {o.needs.map((n, i) => (
              <li key={i}>
                <Badge tone="warn">{n.rule_id}</Badge> {n.reason}
              </li>
            ))}
          </ul>
        </div>
      )
    case 'Rejected':
      return (
        <div className="outcome bad">
          ✖ Rad etildi (log'ga yozildi, auditorga alert):
          <Violations list={o.violations} />
        </div>
      )
  }
}

export function ErrorText({ e }: { e: string | null | undefined }) {
  return e ? <div className="error">{e}</div> : null
}

export function Empty({ children = "Hozircha yo'q" }: { children?: ReactNode }) {
  return <div className="empty">{children}</div>
}

export function Json({ value, label = 'JSON' }: { value: unknown; label?: string }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="json">
      <button className="link" onClick={() => setOpen(!open)}>
        {open ? '▾' : '▸'} {label}
      </button>
      {open && <pre>{typeof value === 'string' ? value : JSON.stringify(value, null, 2)}</pre>}
    </div>
  )
}

export function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {hint && <small>{hint}</small>}
    </label>
  )
}

/** Amal natijasi va xatosini boshqaradi. */
export function useAction() {
  const [busy, setBusy] = useState(false)
  const [outcome, setOutcome] = useState<Outcome | null>(null)
  const [error, setError] = useState<string | null>(null)
  async function run<T>(fn: () => Promise<T>, pick?: (r: T) => Outcome | null | undefined): Promise<T | null> {
    setBusy(true)
    setError(null)
    setOutcome(null)
    try {
      const r = await fn()
      const o = pick ? pick(r) : (r as { outcome?: Outcome })?.outcome
      if (o) setOutcome(o)
      return r
    } catch (e) {
      setError((e as Error).message)
      return null
    } finally {
      setBusy(false)
    }
  }
  return { run, busy, outcome, error, clear: () => (setOutcome(null), setError(null)) }
}

export function ActionResult({ a }: { a: ReturnType<typeof useAction> }) {
  return (
    <>
      <OutcomeView o={a.outcome} />
      <ErrorText e={a.error} />
    </>
  )
}
