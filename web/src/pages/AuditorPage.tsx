import { useState } from 'react'
import type { AuditorView, Movement, PendingApproval } from '../api'
import { api, useApi } from '../api'
import { AnchorsTable, IncidentsList, LedgerFeed, RejectedList, VerifyPanel, useMeta } from '../components/shared'
import { ActionResult, Badge, Card, Empty, ErrorText, Json, Money, PaymentBadge, Tile, Tiles, useAction } from '../components/ui'
import { bps, classLabel, dt, kindLabel, short, som } from '../format'

export default function AuditorPage() {
  const { data, error, reload } = useApi<{ view: AuditorView; integrity_error: string | null; bank_balance: number }>('/api/auditor', 4000)
  if (error) return <ErrorText e={error} />
  if (!data) return <div className="muted">Yuklanmoqda…</div>
  const v = data.view
  const reconOk = v.expected_bank_balance === data.bank_balance
  return (
    <div className="page">
      <h1>Sharia auditor</h1>
      <p className="muted">Real-time ledger, rad etilgan urinishlar, imzo navbati. Rad etilgan urinishning o'zi ham signal.</p>
      <Tiles>
        {v.balances.map((b) => (
          <Tile key={b.class} label={classLabel[b.class]} value={<Money v={b.amount} />} />
        ))}
        <Tile label="Ledger bo'yicha bankda" value={<Money v={v.expected_bank_balance} />} />
        <Tile label="Bank qoldig'i" value={<Money v={data.bank_balance} />} tone={reconOk ? 'good' : 'bad'} note={reconOk ? 'mos (I11)' : 'farq bor — reconciliation'} />
        <Tile label="Rad etilgan urinishlar" value={v.rejected.length} tone={v.rejected.length ? 'warn' : undefined} />
        <Tile label="Imzo kutmoqda" value={v.pending.length} tone={v.pending.length ? 'warn' : undefined} />
      </Tiles>

      <Card title="Tasdiq navbati (I8 related party, I9 rezerv)">
        {v.pending.length === 0 ? <Empty>Imzo kutayotgan tx yo'q</Empty> : v.pending.map((p) => <PendingItem key={p.proposal.id} p={p} onDone={reload} />)}
      </Card>

      <Policies v={v} onDone={reload} />

      <Card title={`Rad etilgan urinishlar (${v.rejected.length})`}>
        <RejectedList list={v.rejected} />
      </Card>

      <div className="two-col">
        <Card title="Rezerv harakati"><Movements list={v.reserve_movements} /></Card>
        <Card title="Xayriya (kechikish jarimalari) oqimi"><Movements list={v.charity_movements} /></Card>
      </div>

      <Card title="Related-party o'tkazmalar">
        {v.related_party_payments.length === 0 ? <Empty /> : (
          <table>
            <thead><tr><th>Vaqt</th><th>Oluvchi</th><th>Summa</th><th>Maqsad</th><th>Holat</th></tr></thead>
            <tbody>
              {v.related_party_payments.map((p) => (
                <tr key={p.payment.id}>
                  <td className="muted">{dt(p.payment.created_at)}</td>
                  <td>{p.beneficiary_name}</td>
                  <td><Money v={p.payment.amount} /></td>
                  <td>{p.payment.purpose}</td>
                  <td><PaymentBadge s={p.status} /></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <VerifyPanel />
      <Card title="Kunlik Merkle root (Sharia board va regulyatorga yuboriladi)"><AnchorsTable anchors={v.anchors} /></Card>
      <Card title="Incident'lar"><IncidentsList list={v.incidents} /></Card>

      <Card title="Muzlatish ishlari (freeze ≠ reversal)">
        {v.freezes.length === 0 ? <Empty /> : (
          <table>
            <thead><tr><th>Ochilgan</th><th>Sabab</th><th>Muzlatilgan lotlar</th><th>Belgilangan (vijdonli)</th><th>Holat</th></tr></thead>
            <tbody>
              {v.freezes.map((f) => (
                <tr key={f.id}>
                  <td className="muted">{dt(f.opened_at)}</td>
                  <td>{f.reason}</td>
                  <td>{f.lots.length}</td>
                  <td>{f.flagged.length}</td>
                  <td>{f.closed_at ? <Badge tone="good">yopilgan</Badge> : <Badge tone="bad">faol</Badge>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <Card title="Tasdiqlar tarixi">
        {v.approval_history.length === 0 ? <Empty /> : (
          <table>
            <thead><tr><th>Vaqt</th><th>Tx</th><th>Natija</th><th>Imzolar</th></tr></thead>
            <tbody>
              {v.approval_history.map((h) => (
                <tr key={h.pending.proposal.id}>
                  <td className="muted">{dt(h.at)}</td>
                  <td>{kindLabel[h.pending.proposal.kind]}: {h.pending.proposal.purpose}</td>
                  <td><Badge tone={h.outcome === 'Executed' ? 'good' : 'bad'}>{h.outcome}</Badge> {h.note}</td>
                  <td>{h.pending.approvals.map((a) => <code key={a.signature}>{a.signer}:{short(a.signature, 10)} </code>)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <LedgerFeed />
    </div>
  )
}

function PendingItem({ p, onDone }: { p: PendingApproval; onDone: () => void }) {
  const { name } = useMeta()
  const [reason, setReason] = useState('')
  const a = useAction()
  const total = p.proposal.legs.reduce((s, l) => s + l.amount, 0)
  const recipients = [...new Set(p.proposal.legs.map((l) => (l.dest.type === 'External' ? name(l.dest.party) : classLabel[l.dest.class])))]
  return (
    <div className="list-item warn-left">
      <div className="row-between">
        <strong>{kindLabel[p.proposal.kind]}: {p.proposal.purpose} — {som(total)}</strong>
        <span className="muted">{dt(p.created_at)}</span>
      </div>
      <div>Oluvchi: {recipients.join(', ')}</div>
      {p.needs.map((n, i) => <div key={i}><Badge tone="warn">{n.rule_id}</Badge> {n.reason}</div>)}
      <div className="actions">
        <button className="primary" disabled={a.busy} onClick={() => a.run(() => api.post(`/api/approvals/${p.proposal.id}/approve`)).then((r) => r && onDone())}>
          Imzolash (Sharia board)
        </button>
        <input placeholder="Rad etish sababi" value={reason} onChange={(e) => setReason(e.target.value)} />
        <button className="danger" disabled={!reason || a.busy} onClick={() => a.run(() => api.post(`/api/approvals/${p.proposal.id}/deny`, { reason })).then((r) => r && onDone())}>
          Rad etish
        </button>
      </div>
      <Json value={p.proposal} label="taklif" />
      <ActionResult a={a} />
    </div>
  )
}

function Policies({ v, onDone }: { v: AuditorView; onDone: () => void }) {
  const { name } = useMeta()
  const a = useAction()
  return (
    <Card title="Policy versiyalari">
      <table>
        <thead><tr><th>Versiya</th><th>Rezerv cap</th><th>Mudarib ulushi</th><th>Related party</th><th>Xayriya oluvchilar</th><th>Imzo</th></tr></thead>
        <tbody>
          {v.policies.map((r) => (
            <tr key={r.policy.version}>
              <td>v{r.policy.version} {v.active_policy === r.policy.version && <Badge tone="good">faol</Badge>}</td>
              <td>{bps(r.policy.reserve_cap_bps)}</td>
              <td>{bps(r.policy.mudarib_share_bps)}</td>
              <td>{r.policy.related_parties.map(name).join(', ') || '—'}</td>
              <td>{r.policy.approved_charities.map(name).join(', ') || '—'}</td>
              <td>
                {r.activation ? (
                  <code title={r.activation.signature}>{r.activation.signer} · {short(r.activation.signature, 12)}</code>
                ) : (
                  <button onClick={() => a.run(() => api.post(`/api/policies/${r.policy.version}/approve`)).then((x) => x && onDone())}>Imzolab faollashtirish</button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <ActionResult a={a} />
    </Card>
  )
}

function Movements({ list }: { list: Movement[] }) {
  if (!list.length) return <Empty />
  return (
    <table>
      <thead><tr><th>Vaqt</th><th>Turi</th><th>Kirim/chiqim</th><th>Summa</th></tr></thead>
      <tbody>
        {list.slice(0, 30).map((m, i) => (
          <tr key={i}>
            <td className="muted">{dt(m.at)}</td>
            <td>{kindLabel[m.kind]}</td>
            <td>{m.direction === 'in' ? <Badge tone="good">kirim</Badge> : <Badge tone="warn">chiqim</Badge>}</td>
            <td><Money v={m.amount} /></td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
