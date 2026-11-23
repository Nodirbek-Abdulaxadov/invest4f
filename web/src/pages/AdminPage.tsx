import { useState } from 'react'
import type { ContractView, Health, Lot, LotStatus, PartyKind, StatementLine, VerifyReport } from '../api'
import { api, useApi } from '../api'
import { PartySelect, useMeta } from '../components/shared'
import { ActionResult, Badge, Card, Empty, ErrorText, Field, Money, useAction } from '../components/ui'
import { classLabel, day, dt, short, som, toTiyin } from '../format'

/** Dev: bank simulyatori va nazorat amallari (faqat OMONAT_DEV=1). */
export default function AdminPage() {
  const { reload: reloadMeta, parties } = useMeta()
  const health = useApi<Health>('/api/health', 4000)
  const bank = useApi<{ balance: number; statement: StatementLine[]; rejected_orders: [string, string][] }>('/api/bank', 4000)
  const seed = useAction()
  if (health.data && !health.data.dev) return <ErrorText e="Dev rejimi o'chirilgan (OMONAT_DEV=1)" />
  return (
    <div className="page">
      <h1>Dev / bank simulyatori</h1>
      <p className="muted">Hamkor bank o'rnida mock bank: nominal hisob, 2-of-2 mandat (operator + Omonat imzosi). Bu sahifa faqat dev rejimida.</p>

      {parties.length === 0 && (
        <Card title="Demo ma'lumotlar">
          <p>Ledger bo'sh. plan.md §3 ssenariysi bo'yicha investorlar, policy, depozitlar va shartnomalarni yaratish — hammasi oddiy yo'l bilan (bank kirimi → Policy Engine → outbox).</p>
          <button className="primary" disabled={seed.busy} onClick={() => seed.run(() => api.post('/api/dev/seed'), () => ({ status: 'Recorded' })).then((r) => r && reloadMeta())}>
            Demo ma'lumotlarni yaratish
          </button>
          <ActionResult a={seed} />
        </Card>
      )}

      <div className="two-col">
        <Incoming onDone={bank.reload} />
        <Controls onDone={bank.reload} today={health.data?.today} />
      </div>

      <Card title={`Bank hisobi — qoldiq ${som(bank.data?.balance)}`}>
        <ErrorText e={bank.error} />
        {bank.data && bank.data.statement.length === 0 ? <Empty /> : (
          <table>
            <thead><tr><th>Ref</th><th>Vaqt</th><th /><th>Summa</th><th>Kontragent</th><th>Izoh</th></tr></thead>
            <tbody>
              {bank.data?.statement.map((l) => (
                <tr key={l.bank_ref}>
                  <td><code>{l.bank_ref}</code></td>
                  <td className="muted">{dt(l.at)}</td>
                  <td>{l.direction === 'In' ? <Badge tone="good">kirim</Badge> : <Badge tone="warn">chiqim</Badge>}</td>
                  <td><Money v={l.amount} /></td>
                  <td>{l.counterparty}</td>
                  <td>{l.memo}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {bank.data && bank.data.rejected_orders.length > 0 && (
          <>
            <h4>Bank rad etgan topshiriqlar</h4>
            <ul>{bank.data.rejected_orders.map(([id, r]) => <li key={id}><code>{short(id)}</code>: {r}</li>)}</ul>
          </>
        )}
      </Card>

      <div className="two-col">
        <FreezePanel />
        <NewParty onDone={reloadMeta} />
      </div>
      <Tamper store={health.data?.store} />
    </div>
  )
}

function Incoming({ onDone }: { onDone: () => void }) {
  const [kind, setKind] = useState<'deposit' | 'installment'>('deposit')
  const [party, setParty] = useState('')
  const [contract, setContract] = useState('')
  const [amount, setAmount] = useState('')
  const [fee, setFee] = useState('')
  const contracts = useApi<ContractView[]>('/api/contracts', 0)
  const a = useAction()
  const active = (contracts.data ?? []).filter((c) => c.effective_status === 'Active' || c.effective_status === 'Overdue')
  const chosen = active.find((c) => c.contract.id === contract)
  const next = chosen?.contract.schedule.find((_, i, s) => s.slice(0, i + 1).reduce((x, y) => x + y.principal + y.markup, 0) > chosen.contract.paid_principal + chosen.contract.paid_markup)
  async function submit() {
    const body = kind === 'deposit'
      ? { kind, party, amount: toTiyin(amount) }
      : { kind, party: chosen?.contract.customer, contract_id: contract, amount: toTiyin(amount), late_fee: fee ? toTiyin(fee) : 0 }
    const r = await a.run(() => api.post('/api/dev/bank/incoming', body))
    if (r) {
      onDone()
      contracts.reload()
    }
  }
  return (
    <Card title="Nominal hisobga kirim">
      <div className="form-row">
        <Field label="Turi">
          <select value={kind} onChange={(e) => setKind(e.target.value as 'deposit' | 'installment')}>
            <option value="deposit">Investor depoziti</option>
            <option value="installment">Mijozning muddatli to'lovi</option>
          </select>
        </Field>
        {kind === 'deposit' ? (
          <Field label="Investor"><PartySelect kind="Investor" value={party} onChange={setParty} /></Field>
        ) : (
          <Field label="Shartnoma" hint={next && <>navbatdagi: {day(next.due)} · {som(next.principal + next.markup)}</>}>
            <select value={contract} onChange={(e) => setContract(e.target.value)} onFocus={() => contracts.reload()}>
              <option value="">— tanlang —</option>
              {active.map((c) => <option key={c.contract.id} value={c.contract.id}>{c.contract.goods} ({c.customer_name})</option>)}
            </select>
          </Field>
        )}
        <Field label="Summa (so'm)"><input value={amount} onChange={(e) => setAmount(e.target.value)} /></Field>
        {kind === 'installment' && <Field label="Shundan jarima (so'm)"><input value={fee} onChange={(e) => setFee(e.target.value)} /></Field>}
      </div>
      <button className="primary" disabled={!(toTiyin(amount) > 0) || (kind === 'deposit' ? !party : !contract) || a.busy} onClick={submit}>Bankka tushirish</button>
      <ActionResult a={a} />
    </Card>
  )
}

function Controls({ onDone, today }: { onDone: () => void; today?: string }) {
  const a = useAction()
  const [delta, setDelta] = useState('')
  const [days, setDays] = useState('35')
  const run = (fn: () => Promise<unknown>) => a.run(fn).then((r) => r && onDone())
  return (
    <Card title="Nazorat">
      <div className="stack">
        <button onClick={() => run(() => api.post('/api/reconcile'))}>Reconciliation (ledger ↔ bank, I11)</button>
        <button onClick={() => run(() => api.post('/api/anchors', { day: today }))}>Bugungi Merkle root'ni e'lon qilish</button>
        <div className="form-row">
          <Field label="Bankda qayd etilmagan harakat (so'm, ±)" hint="I11 namoyishi: keyingi reconciliation incident ochadi">
            <input value={delta} onChange={(e) => setDelta(e.target.value)} placeholder="-150000" />
          </Field>
          <button disabled={!delta} onClick={() => {
            const neg = delta.trim().startsWith('-')
            const t = toTiyin(delta.replace('-', ''))
            run(() => api.post('/api/dev/bank/adjust', { delta: neg ? -t : t, memo: "Noma'lum o'tkazma" }))
          }}>Qo'llash</button>
        </div>
        <button onClick={() => run(() => api.post('/api/dev/bank/decline-next', { reason: 'Oluvchi hisobi bloklangan' }))}>
          Navbatdagi to'lovni bank rad etsin (Compensation)
        </button>
        <div className="form-row">
          <Field label="Vaqtni oldinga surish (kun)" hint="Muddati o'tgan to'lovlarni ko'rish uchun">
            <input value={days} onChange={(e) => setDays(e.target.value)} />
          </Field>
          <button onClick={() => run(() => api.post('/api/dev/clock', { days: Number(days) }))}>Surish</button>
        </div>
      </div>
      <ActionResult a={a} />
    </Card>
  )
}

function FreezePanel() {
  const { ofKind, name } = useMeta()
  const [owner, setOwner] = useState('')
  const lots = useApi<{ lot: Lot; status: LotStatus }[]>(owner ? `/api/lots?owner=${owner}&all=true` : null, 0)
  const [lot, setLot] = useState('')
  const [reason, setReason] = useState("O'g'irlangan kartadan depozit shubhasi")
  const a = useAction()
  return (
    <Card title="Tracing & freeze">
      <p className="muted">Depozit lotidan FIFO forward trace; faqat shu egasining faol lotlari muzlatiladi, boshqalar belgilanadi (vijdonli oluvchi).</p>
      <div className="form-row">
        <Field label="Investor">
          <select value={owner} onChange={(e) => { setOwner(e.target.value); setLot('') }}>
            <option value="">—</option>
            {ofKind('Investor').map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </Field>
        <Field label="Ildiz lot">
          <select value={lot} onChange={(e) => setLot(e.target.value)}>
            <option value="">—</option>
            {lots.data?.filter((l) => l.lot.parent_id === null).map((l) => (
              <option key={l.lot.id} value={l.lot.id}>{classLabel[l.lot.class]} {som(l.lot.amount)} · {dt(l.lot.created_at)}</option>
            ))}
          </select>
        </Field>
      </div>
      <Field label="Sabab"><input value={reason} onChange={(e) => setReason(e.target.value)} /></Field>
      <button className="danger" disabled={!lot || a.busy} onClick={() => a.run(() => api.post('/api/freeze', { lot_id: lot, reason }))}>Muzlatish</button>
      <ActionResult a={a} />
      {owner && <p className="muted small">{name(owner)}: muzlatilgan ishlarni auditor sahifasida ko'ring.</p>}
    </Card>
  )
}

function NewParty({ onDone }: { onDone: () => void }) {
  const [kind, setKind] = useState<PartyKind>('Investor')
  const [nm, setNm] = useState('')
  const [acct, setAcct] = useState('')
  const a = useAction()
  return (
    <Card title="Ishtirokchini ro'yxatga olish">
      <div className="form-row">
        <Field label="Turi">
          <select value={kind} onChange={(e) => setKind(e.target.value as PartyKind)}>
            {(['Investor', 'Merchant', 'Customer', 'Vendor', 'Charity'] as PartyKind[]).map((k) => <option key={k}>{k}</option>)}
          </select>
        </Field>
        <Field label="Nomi"><input value={nm} onChange={(e) => setNm(e.target.value)} /></Field>
        <Field label="Hisob raqami"><input value={acct} onChange={(e) => setAcct(e.target.value)} /></Field>
      </div>
      <button disabled={!nm || !acct || a.busy} onClick={() => a.run(() => api.post('/api/parties', { kind, name: nm, bank_account: acct }), () => ({ status: 'Recorded' })).then((r) => r && (setNm(''), setAcct(''), onDone()))}>
        Qo'shish
      </button>
      <ActionResult a={a} />
    </Card>
  )
}

function Tamper({ store }: { store?: string }) {
  const [seq, setSeq] = useState('')
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const [report, setReport] = useState<VerifyReport | null>(null)
  const [err, setErr] = useState<string | null>(null)
  async function run() {
    setErr(null)
    try {
      setReport(await api.post<VerifyReport>('/api/dev/tamper', { seq: Number(seq), from, to }))
    } catch (e) {
      setErr((e as Error).message)
    }
  }
  return (
    <Card title="I12 namoyishi: ledger yozuvini «qo'lda» o'zgartirish">
      {store === 'postgres' ? (
        <p className="muted">
          PostgreSQL'da bu amal mumkin emas: ilova rolida UPDATE/DELETE huquqi yo'q va trigger har qanday o'zgartirishni bloklaydi.
          Superuser trigger'ni o'chirib o'zgartirsa, «Zanjirni tekshirish» buzilishni aniqlaydi.
        </p>
      ) : (
        <>
          <p className="muted">Xotiradagi store'da yozuv payload'ini almashtiradi va darhol tekshiradi. Buzilish aniqlangach yozish to'xtatiladi (server qayta ishga tushguncha).</p>
          <div className="form-row">
            <Field label="seq"><input value={seq} onChange={(e) => setSeq(e.target.value)} /></Field>
            <Field label="Qidirish (masalan summa)"><input value={from} onChange={(e) => setFrom(e.target.value)} /></Field>
            <Field label="Almashtirish"><input value={to} onChange={(e) => setTo(e.target.value)} /></Field>
            <button className="danger" disabled={!seq || !from} onClick={run}>O'zgartirish</button>
          </div>
        </>
      )}
      <ErrorText e={err} />
      {report && (
        <p>
          Zanjir: {report.chain_ok ? <Badge tone="good">butun</Badge> : <Badge tone="bad">buzilgan</Badge>} {report.chain_error}
        </p>
      )}
    </Card>
  )
}
