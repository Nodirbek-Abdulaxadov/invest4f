import { useState } from 'react'
import type { ClassBalance, ContractView, Lot, LotStatus, OutboxItem, PendingApproval, RejectedAttempt, TxProposal, TxView } from '../api'
import { api, useApi } from '../api'
import { PartySelect, RejectedList, useMeta } from '../components/shared'
import { ActionResult, Badge, Card, Empty, ErrorText, Field, Json, Money, PaymentBadge, StatusBadge, Tile, Tiles, useAction } from '../components/ui'
import { classLabel, day, dt, kindLabel, short, som, toTiyin } from '../format'

interface OperatorView {
  balances: ClassBalance[]
  contracts: ContractView[]
  pending: PendingApproval[]
  rejected: RejectedAttempt[]
  txs: TxView[]
  outbox: OutboxItem[]
}

export default function OperatorPage() {
  const { data, error, reload } = useApi<OperatorView>('/api/operator')
  const [tab, setTab] = useState<'contracts' | 'payments' | 'raw' | 'log'>('contracts')
  return (
    <div className="page">
      <h1>Operator (mudarib)</h1>
      <p className="muted">Har bir taklif bajarilishidan oldin Policy Engine (I1–I13) tekshiradi. Bankka to'lov faqat Omonat imzosi bilan chiqadi.</p>
      <ErrorText e={error} />
      {data && (
        <Tiles>
          {data.balances.map((b) => (
            <Tile key={b.class} label={classLabel[b.class]} value={<Money v={b.amount} />} />
          ))}
        </Tiles>
      )}
      <div className="tabs">
        <button className={tab === 'contracts' ? 'active' : ''} onClick={() => setTab('contracts')}>Shartnomalar</button>
        <button className={tab === 'payments' ? 'active' : ''} onClick={() => setTab('payments')}>To'lov takliflari</button>
        <button className={tab === 'raw' ? 'active' : ''} onClick={() => setTab('raw')}>Qo'lda taklif</button>
        <button className={tab === 'log' ? 'active' : ''} onClick={() => setTab('log')}>
          Natijalar {data && data.pending.length > 0 && <Badge tone="warn">{data.pending.length}</Badge>}
        </button>
      </div>
      {tab === 'contracts' && (
        <>
          <NewContract onDone={reload} />
          {data?.contracts.map((c) => <ContractCard key={c.contract.id} v={c} onDone={reload} />)}
        </>
      )}
      {tab === 'payments' && <Payments onDone={reload} />}
      {tab === 'raw' && <RawProposal onDone={reload} />}
      {tab === 'log' && data && <Log v={data} />}
    </div>
  )
}

function NewContract({ onDone }: { onDone: () => void }) {
  const [merchant, setMerchant] = useState('')
  const [customer, setCustomer] = useState('')
  const [goods, setGoods] = useState('')
  const [cost, setCost] = useState('')
  const [markup, setMarkup] = useState('')
  const [n, setN] = useState('6')
  const [due, setDue] = useState(() => new Date(Date.now() + 30 * 86400e3).toISOString().slice(0, 10))
  const a = useAction()
  const ok = merchant && customer && goods && toTiyin(cost) > 0 && toTiyin(markup) >= 0 && Number(n) > 0
  async function submit() {
    const r = await a.run(() =>
      api.post('/api/contracts', {
        merchant, customer, goods, cost: toTiyin(cost), markup: toTiyin(markup), installments: Number(n), first_due: due,
      }),
    )
    if (r) onDone()
  }
  return (
    <Card title="Yangi murabaha shartnomasi">
      <div className="form-grid">
        <Field label="Merchant (tovar sotuvchi)"><PartySelect kind="Merchant" value={merchant} onChange={setMerchant} /></Field>
        <Field label="Mijoz"><PartySelect kind="Customer" value={customer} onChange={setCustomer} /></Field>
        <Field label="Tovar"><input value={goods} onChange={(e) => setGoods(e.target.value)} /></Field>
        <Field label="Tannarx (so'm)"><input value={cost} onChange={(e) => setCost(e.target.value)} /></Field>
        <Field label="Ustama (so'm)"><input value={markup} onChange={(e) => setMarkup(e.target.value)} /></Field>
        <Field label="Oylar soni"><input value={n} onChange={(e) => setN(e.target.value)} /></Field>
        <Field label="Birinchi to'lov sanasi"><input type="date" value={due} onChange={(e) => setDue(e.target.value)} /></Field>
      </div>
      <button className="primary" disabled={!ok || a.busy} onClick={submit}>Shartnoma tuzish (Draft)</button>
      <ActionResult a={a} />
    </Card>
  )
}

function ContractCard({ v, onDone }: { v: ContractView; onDone: () => void }) {
  const { name } = useMeta()
  const c = v.contract
  const [open, setOpen] = useState(false)
  const [reserve, setReserve] = useState('5')
  const [fault, setFault] = useState<'None' | 'Mudarib'>('None')
  const [docNo, setDocNo] = useState('')
  const a = useAction()
  const st = v.effective_status
  const post = (path: string, body: unknown = {}) =>
    a.run(() => api.post(`/api/contracts/${c.id}/${path}`, body)).then((r) => r && onDone())
  return (
    <Card
      title={
        <>
          {c.goods} <StatusBadge s={st} />
        </>
      }
      actions={<button className="link" onClick={() => setOpen(!open)}>{open ? 'Yopish' : 'Batafsil'}</button>}
    >
      <div className="kv">
        <span>Merchant: {v.merchant_name}</span>
        <span>Mijoz: {v.customer_name}</span>
        <span>Tannarx: {som(c.cost)}</span>
        <span>Ustama: {som(c.markup)}</span>
        <span>To'langan: {som(c.paid_principal)} + {som(c.paid_markup)}</span>
        <span>Bugungacha kerak: {som(v.due_until_today)}</span>
        {c.late_fees > 0 && <span>Jarima (xayriyaga): {som(c.late_fees)}</span>}
        {v.profit_undistributed > 0 && <span className="warn-text">Taqsimlanmagan foyda: {som(v.profit_undistributed)}</span>}
        <span>Egalik hujjati: {c.ownership_doc ? <Badge tone="good">{c.ownership_doc.doc_no}</Badge> : <Badge>yo'q</Badge>}</span>
      </div>
      <div className="actions">
        {st === 'Draft' && !c.purchase_tx && <button onClick={() => post('deploy')}>Tovar xaridi (kapitalni taqsimlash)</button>}
        {(st === 'Draft' || st === 'GoodsPurchased') && !c.ownership_doc && (
          <span className="inline-form">
            <input placeholder="Qabz raqami" value={docNo} onChange={(e) => setDocNo(e.target.value)} />
            <button disabled={!docNo} onClick={() => post('ownership-doc', { doc_no: docNo, description: 'Merchant qabzi', content: `${docNo}|${c.id}` })}>
              Egalik hujjatini biriktirish
            </button>
          </span>
        )}
        {(st === 'Draft' || st === 'GoodsPurchased') && <button onClick={() => post('activate')}>Mijozga sotish (faollashtirish)</button>}
        {(st === 'Active' || st === 'Overdue' || st === 'Repaid') && v.profit_undistributed > 0 && (
          <span className="inline-form">
            <input style={{ width: 60 }} value={reserve} onChange={(e) => setReserve(e.target.value)} />%
            <button onClick={() => post('profit-split', { reserve_bps: Math.round(Number(reserve) * 100) })}>Foydani taqsimlash</button>
          </span>
        )}
        {st === 'Overdue' && (
          <span className="inline-form">
            <select value={fault} onChange={(e) => setFault(e.target.value as 'None' | 'Mudarib')}>
              <option value="None">Mijoz aybi</option>
              <option value="Mudarib">Mudarib aybi (ta'addi/taqsir)</option>
            </select>
            <button className="danger" onClick={() => post('default', { fault })}>Defolt e'lon qilish</button>
          </span>
        )}
        {st === 'Defaulted' && <button onClick={() => post('reserve-cover', {})}>Rezervdan qoplashni taklif qilish</button>}
      </div>
      <ActionResult a={a} />
      {open && (
        <div className="two-col">
          <div>
            <h4>Pozitsiyalar</h4>
            {v.positions.length === 0 ? <Empty /> : (
              <table>
                <thead><tr><th>Investor</th><th>Kiritgan</th><th>Qaytgan</th><th>Zarar</th><th>Qoplangan</th></tr></thead>
                <tbody>
                  {v.positions.map((p) => (
                    <tr key={p.investor}><td>{name(p.investor)}</td><td><Money v={p.principal} /></td><td><Money v={p.repaid} /></td><td><Money v={p.written_off} /></td><td><Money v={p.covered} /></td></tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
          <div>
            <h4>To'lov jadvali</h4>
            <table>
              <thead><tr><th>#</th><th>Sana</th><th>Asosiy</th><th>Ustama</th></tr></thead>
              <tbody>
                {c.schedule.map((i) => (
                  <tr key={i.no}><td>{i.no}</td><td>{day(i.due)}</td><td><Money v={i.principal} /></td><td><Money v={i.markup} /></td></tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </Card>
  )
}

function PayForm({ title, kind, path, purpose: defPurpose, onDone, note }: {
  title: string; kind: 'Vendor' | 'Charity'; path: string; purpose: string; onDone: () => void; note: string
}) {
  const [party, setParty] = useState('')
  const [amount, setAmount] = useState('')
  const [purpose, setPurpose] = useState(defPurpose)
  const a = useAction()
  return (
    <Card title={title}>
      <p className="muted">{note}</p>
      <div className="form-row">
        <Field label="Oluvchi"><PartySelect kind={kind} value={party} onChange={setParty} /></Field>
        <Field label="Summa (so'm)"><input value={amount} onChange={(e) => setAmount(e.target.value)} /></Field>
        <Field label="Maqsad"><input value={purpose} onChange={(e) => setPurpose(e.target.value)} /></Field>
        <button disabled={!party || !(toTiyin(amount) > 0) || a.busy}
          onClick={() => a.run(() => api.post(path, { party, amount: toTiyin(amount), purpose })).then((r) => r && onDone())}>
          Taklif qilish
        </button>
      </div>
      <ActionResult a={a} />
    </Card>
  )
}

function Payments({ onDone }: { onDone: () => void }) {
  const [inv, setInv] = useState('')
  const [amount, setAmount] = useState('')
  const [winv, setWinv] = useState('')
  const [wamount, setWamount] = useState('')
  const a = useAction()
  const w = useAction()
  return (
    <>
      <Card title="Dividend">
        <p className="muted">Faqat investorning InvestorProfit lotlaridan (I1). Summa bo'sh bo'lsa — barcha to'lanmagan foyda.</p>
        <div className="form-row">
          <Field label="Investor"><PartySelect kind="Investor" value={inv} onChange={setInv} /></Field>
          <Field label="Summa (so'm, ixtiyoriy)"><input value={amount} onChange={(e) => setAmount(e.target.value)} /></Field>
          <button disabled={!inv || a.busy}
            onClick={() => a.run(() => api.post('/api/dividends', { investor: inv, amount: amount ? toTiyin(amount) : null })).then((r) => r && onDone())}>
            Dividend to'lash
          </button>
        </div>
        <ActionResult a={a} />
      </Card>
      <Card title="Investor nomidan yechib olish">
        <div className="form-row">
          <Field label="Investor"><PartySelect kind="Investor" value={winv} onChange={setWinv} /></Field>
          <Field label="Summa (so'm)"><input value={wamount} onChange={(e) => setWamount(e.target.value)} /></Field>
          <button disabled={!winv || !(toTiyin(wamount) > 0) || w.busy}
            onClick={() => w.run(() => api.post('/api/withdrawals', { investor: winv, amount: toTiyin(wamount) })).then((r) => r && onDone())}>
            Yechib olish
          </button>
        </div>
        <ActionResult a={w} />
      </Card>
      <PayForm title="Operatsion xarajat" kind="Vendor" path="/api/expenses" purpose="Ofis ijarasi" onDone={onDone}
        note="Faqat MudaribShare'dan (I3). Related-party oluvchiga — Sharia board imzosi kerak (I8)." />
      <PayForm title="Xayriya" kind="Charity" path="/api/donations" purpose="Kechikish jarimalari" onDone={onDone}
        note="Charity puli faqat Sharia board tasdiqlagan tashkilotga (I7)." />
    </>
  )
}

const TEMPLATES: { label: string; rule: string; build: (lots: { lot: Lot; status: LotStatus }[], ids: { investors: string[]; vendor?: string }) => Partial<TxProposal> | null }[] = [
  {
    label: "Yangi investor kapitalidan eski investorga «dividend»",
    rule: 'I1',
    build: (lots, ids) => {
      const l = lots.find((x) => x.lot.class === 'Capital')
      const other = ids.investors.find((i) => i !== l?.lot.owner)
      return l && other ? { kind: 'Dividend', purpose: 'Oylik dividend', legs: [{ source: { type: 'Lot', lot_id: l.lot.id }, dest: { type: 'External', party: other }, amount: Math.min(l.lot.amount, 50000000) }] } : null
    },
  },
  {
    label: "A investorning yechishi B lotlaridan",
    rule: 'I2',
    build: (lots, ids) => {
      const l = lots.find((x) => x.lot.class === 'Capital')
      const other = ids.investors.find((i) => i !== l?.lot.owner)
      return l && other ? { kind: 'Withdrawal', purpose: 'Kapitalni qaytarish', legs: [{ source: { type: 'Lot', lot_id: l.lot.id }, dest: { type: 'External', party: other }, amount: Math.min(l.lot.amount, 100000000) }] } : null
    },
  },
  {
    label: 'Marketing agentligiga to‘lov Capital’dan',
    rule: 'I3',
    build: (lots, ids) => {
      const l = lots.find((x) => x.lot.class === 'Capital')
      return l && ids.vendor ? { kind: 'Expense', purpose: 'Reklama kampaniyasi', legs: [{ source: { type: 'Lot', lot_id: l.lot.id }, dest: { type: 'External', party: ids.vendor }, amount: Math.min(l.lot.amount, 30000000) }] } : null
    },
  },
]

function RawProposal({ onDone }: { onDone: () => void }) {
  const { ofKind } = useMeta()
  const [text, setText] = useState('')
  const [err, setErr] = useState<string | null>(null)
  const a = useAction()
  async function fill(i: number) {
    setErr(null)
    const lots = await api.get<{ lot: Lot; status: LotStatus }[]>('/api/lots?class=Capital')
    const p = TEMPLATES[i].build(lots, { investors: ofKind('Investor').map((x) => x.id), vendor: ofKind('Vendor')[0]?.id })
    if (!p) return setErr("Namuna uchun mos lot yoki ishtirokchi topilmadi")
    setText(JSON.stringify({ id: crypto.randomUUID(), contract_id: null, write_offs: [], proposed_by: { role: 'Operator' }, ...p }, null, 2))
  }
  async function submit() {
    setErr(null)
    let p: TxProposal
    try {
      p = JSON.parse(text)
    } catch {
      return setErr("JSON noto'g'ri")
    }
    const r = await a.run(() => api.post('/api/tx', p))
    if (r) onDone()
  }
  return (
    <Card title="Qo'lda taklif (TxProposal)">
      <p className="muted">
        Istalgan taklifni to'g'ridan-to'g'ri yuborish mumkin — u ham xuddi shu Policy Engine'dan o'tadi. Quyidagi namunalar
        plan.md §3.4'dagi rad etilishi kerak bo'lgan urinishlar.
      </p>
      <div className="actions">
        {TEMPLATES.map((t, i) => (
          <button key={i} className="ghost" onClick={() => fill(i)}>
            <Badge tone="bad">{t.rule}</Badge> {t.label}
          </button>
        ))}
      </div>
      <textarea rows={14} value={text} onChange={(e) => setText(e.target.value)} spellCheck={false} placeholder='{"kind": "...", "legs": [...]}' />
      <button className="primary" disabled={!text || a.busy} onClick={submit}>Yuborish</button>
      <ErrorText e={err} />
      <ActionResult a={a} />
    </Card>
  )
}

function Log({ v }: { v: OperatorView }) {
  return (
    <>
      <Card title="Tasdiq kutayotgan takliflar">
        {v.pending.length === 0 ? <Empty /> : v.pending.map((p) => (
          <div key={p.proposal.id} className="list-item warn-left">
            <strong>{kindLabel[p.proposal.kind]}</strong>: {p.proposal.purpose} — {som(p.proposal.legs.reduce((s, l) => s + l.amount, 0))}
            {p.needs.map((n, i) => <div key={i} className="muted"><Badge tone="warn">{n.rule_id}</Badge> {n.reason}</div>)}
          </div>
        ))}
      </Card>
      <Card title="Oxirgi tranzaksiyalar">
        <table>
          <thead><tr><th>Vaqt</th><th>Turi</th><th>Maqsad</th><th>Summa</th><th>Bank to'lovlari</th></tr></thead>
          <tbody>
            {v.txs.map((t) => (
              <tr key={t.tx.proposal.id}>
                <td className="muted">{dt(t.tx.at)}</td>
                <td>{kindLabel[t.tx.proposal.kind]}</td>
                <td>{t.tx.proposal.purpose}</td>
                <td><Money v={t.tx.proposal.legs.reduce((s, l) => s + l.amount, 0)} /></td>
                <td>{t.payments.map((p) => <div key={p.payment.id}>{p.beneficiary_name}: {som(p.payment.amount)} <PaymentBadge s={p.status} /></div>)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
      <Card title="Bank Gateway outbox (operator + Omonat HSM imzosi)">
        {v.outbox.length === 0 ? <Empty /> : (
          <table>
            <thead><tr><th>To'lov</th><th>Ledger seq</th><th>Holat</th><th>Bank javobi</th><th /></tr></thead>
            <tbody>
              {v.outbox.map((o) => (
                <tr key={o.id}>
                  <td><code>{short(o.id)}</code></td>
                  <td>{o.ledger_seq}</td>
                  <td><Badge tone={o.status === 'sent' ? 'good' : o.status === 'failed' ? 'bad' : 'info'}>{o.status}</Badge></td>
                  <td className="muted">{o.note}</td>
                  <td><Json value={JSON.parse(o.body)} label="imzolangan order" /></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
      <Card title="Rad etilgan takliflar">
        <RejectedList list={v.rejected} />
      </Card>
    </>
  )
}
