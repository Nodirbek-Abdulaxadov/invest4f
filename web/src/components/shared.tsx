import { createContext, useContext, useState } from 'react'
import type { Anchor, Incident, LedgerEntry, LineageStep, Meta, Party, PartyKind, RejectedAttempt, VerifyReport } from '../api'
import { api, useApi } from '../api'
import { classLabel, dt, day, kindLabel, short, som } from '../format'
import { Badge, Card, Empty, ErrorText, Json, Violations } from './ui'

export const MetaContext = createContext<{ meta: Meta | null; reload: () => void }>({ meta: null, reload: () => {} })

export function useMeta() {
  const { meta, reload } = useContext(MetaContext)
  const parties = meta?.parties ?? []
  const byId = new Map(parties.map((p) => [p.id, p]))
  return {
    meta,
    reload,
    parties,
    name: (id: string | null | undefined) => (id ? byId.get(id)?.name ?? short(id) : 'Fond'),
    ofKind: (k: PartyKind): Party[] => parties.filter((p) => p.kind === k),
  }
}

export function PartySelect({ kind, value, onChange }: { kind: PartyKind; value: string; onChange: (v: string) => void }) {
  const { ofKind } = useMeta()
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      <option value="">— tanlang —</option>
      {ofKind(kind).map((p) => (
        <option key={p.id} value={p.id}>
          {p.name}
        </option>
      ))}
    </select>
  )
}

export function LineageView({ steps }: { steps: LineageStep[] }) {
  const { name } = useMeta()
  return (
    <ol className="lineage">
      {steps.map((s) => (
        <li key={s.lot_id}>
          <Badge tone="info">{classLabel[s.class]}</Badge> {som(s.amount)} · {s.owner ? name(s.owner) : 'fond'}
          {s.tx_kind && <> · {kindLabel[s.tx_kind]}</>}
          {s.bank_ref && <> · bank: <code>{s.bank_ref}</code></>}
          {s.contract_id && <> · shartnoma <code>{short(s.contract_id)}</code></>}
          <span className="muted"> · {dt(s.at)} · lot <code>{short(s.lot_id)}</code></span>
        </li>
      ))}
    </ol>
  )
}

function attemptLabel(r: RejectedAttempt): string {
  switch (r.attempt.type) {
    case 'Tx':
      return `${kindLabel[r.attempt.proposal.kind]}: ${r.attempt.proposal.purpose} (${som(
        r.attempt.proposal.legs.reduce((a, l) => a + l.amount, 0),
      )})`
    case 'ActivateContract':
      return `Shartnomani faollashtirish ${short(r.attempt.contract_id)}`
    case 'Policy':
      return `Policy v${r.attempt.version}`
    case 'Other':
      return r.attempt.action
  }
}

export function RejectedList({ list, limit = 30 }: { list: RejectedAttempt[]; limit?: number }) {
  if (!list.length) return <Empty>Rad etilgan urinish yo'q</Empty>
  return (
    <div className="list">
      {list.slice(0, limit).map((r) => (
        <div key={r.id} className="list-item bad-left">
          <div className="row-between">
            <strong>{attemptLabel(r)}</strong>
            <span className="muted">{dt(r.at)}</span>
          </div>
          <Violations list={r.violations} />
          <Json value={r.attempt} label="urinish" />
        </div>
      ))}
    </div>
  )
}

export function LedgerFeed({ limit = 25 }: { limit?: number }) {
  const { data, error } = useApi<{ total: number; entries: LedgerEntry[] }>(`/api/ledger?limit=${limit}`)
  return (
    <Card title={`Ledger (append-only, hash chain) — ${data?.total ?? 0} yozuv`}>
      <ErrorText e={error} />
      <table>
        <thead>
          <tr>
            <th>seq</th>
            <th>hodisa</th>
            <th>vaqt</th>
            <th>prev_hash</th>
            <th>hash</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {data?.entries.map((e) => (
            <tr key={e.seq}>
              <td>{e.seq}</td>
              <td>{e.kind}</td>
              <td className="muted">{dt(e.at)}</td>
              <td><code>{short(e.prev_hash, 12)}</code></td>
              <td><code>{short(e.hash, 12)}</code></td>
              <td><Json value={JSON.parse(e.payload)} label="payload" /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  )
}

export function VerifyPanel() {
  const [report, setReport] = useState<VerifyReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  async function run() {
    setError(null)
    try {
      setReport(await api.get<VerifyReport>('/api/ledger/verify'))
    } catch (e) {
      setError((e as Error).message)
    }
  }
  return (
    <Card title="Yaxlitlik tekshiruvi (I12)" actions={<button onClick={run}>Zanjir va Merkle root'larni tekshirish</button>}>
      <ErrorText e={error} />
      {report ? (
        <div>
          <p>
            {report.entries} yozuv · zanjir:{' '}
            {report.chain_ok ? <Badge tone="good">butun</Badge> : <Badge tone="bad">buzilgan</Badge>}
            {report.chain_error && <span className="error-inline"> {report.chain_error}</span>} · store = ilova:{' '}
            {report.store_matches_memory ? <Badge tone="good">ha</Badge> : <Badge tone="bad">yo'q</Badge>}
          </p>
          {report.anchors.length > 0 && (
            <table>
              <thead>
                <tr><th>kun</th><th>seq</th><th>e'lon qilingan root</th><th>qayta hisoblangan</th><th /></tr>
              </thead>
              <tbody>
                {report.anchors.map((a) => (
                  <tr key={a.anchor.day}>
                    <td>{day(a.anchor.day)}</td>
                    <td>{a.anchor.from_seq}–{a.anchor.to_seq}</td>
                    <td><code>{short(a.anchor.root, 16)}</code></td>
                    <td><code>{short(a.recomputed_root, 16)}</code></td>
                    <td>{a.ok ? <Badge tone="good">mos</Badge> : <Badge tone="bad">mos emas</Badge>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      ) : (
        <p className="muted">Store'dagi yozuvlar qayta o'qilib, hash chain va e'lon qilingan kunlik Merkle root'lar bilan solishtiriladi.</p>
      )}
    </Card>
  )
}

export function AnchorsTable({ anchors }: { anchors: Anchor[] }) {
  if (!anchors.length) return <Empty>Hali e'lon qilingan root yo'q (har kuni yarim tundan keyin)</Empty>
  return (
    <table>
      <thead>
        <tr><th>kun</th><th>yozuvlar</th><th>Merkle root</th><th>Omonat imzosi</th><th>vaqt</th></tr>
      </thead>
      <tbody>
        {anchors.map((a) => (
          <tr key={a.day}>
            <td>{day(a.day)}</td>
            <td>{a.from_seq}–{a.to_seq}</td>
            <td><code title={a.root}>{short(a.root, 20)}</code></td>
            <td><code title={a.signature}>{short(a.signature, 16)}</code></td>
            <td className="muted">{dt(a.at)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

export function IncidentsList({ list }: { list: Incident[] }) {
  if (!list.length) return <Empty>Incident yo'q</Empty>
  return (
    <div className="list">
      {list.map((i) => (
        <div key={i.id} className="list-item bad-left">
          <div className="row-between">
            <Badge tone="bad">{i.kind}</Badge>
            <span className="muted">{dt(i.at)}</span>
          </div>
          <div>{i.details}</div>
        </div>
      ))}
    </div>
  )
}
