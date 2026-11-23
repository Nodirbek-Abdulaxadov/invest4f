import { useState } from 'react'
import type { InvestorDashboard, WithdrawalPlan } from '../api'
import { api, useApi } from '../api'
import { LineageView } from '../components/shared'
import { ActionResult, Badge, Card, Empty, ErrorText, Field, Money, PaymentBadge, StatusBadge, Tile, Tiles, useAction } from '../components/ui'
import { bps, day, dt, som, toTiyin } from '../format'

export default function InvestorPage({ id }: { id: string }) {
  const { data: d, error, reload } = useApi<InvestorDashboard>(`/api/investors/${id}/dashboard`)
  const [open, setOpen] = useState<string | null>(null)
  if (error) return <ErrorText e={error} />
  if (!d) return <div className="muted">Yuklanmoqda…</div>
  return (
    <div className="page">
      <h1>{d.investor.name}</h1>
      <p className="muted">Mudaraba investitsiya hisobi · hisob raqami {d.investor.bank_account}</p>
      <Tiles>
        <Tile label="Jami kiritilgan" value={<Money v={d.deposited} />} />
        <Tile label="Likvid (bo'sh kapital)" value={<Money v={d.liquid} />} note="Hozir yechib olish mumkin" />
        <Tile label="Faol (shartnomalarda)" value={<Money v={d.active} />} note={`${d.positions.length} ta pozitsiya`} />
        <Tile label="Qaytgan asosiy qism" value={<Money v={d.returned_principal} />} />
        <Tile label="Olingan foyda" value={<Money v={d.profit_credited} />} note={<>to'langan dividend: {som(d.dividends_paid)}</>} tone="good" />
        <Tile label="To'lanmagan foyda" value={<Money v={d.profit_available} />} />
        <Tile
          label="Kutilayotgan foyda"
          value={<Money v={d.expected_profit} />}
          note={<Badge tone="warn">{d.expected_profit_label}</Badge>}
          tone="muted"
        />
        <Tile label="Muammoli (muddati o'tgan)" value={<Money v={d.overdue} />} note={`${d.overdue_contracts} ta shartnoma`} tone={d.overdue > 0 ? 'warn' : undefined} />
        <Tile label="Defolt zarari" value={<Money v={d.defaulted_loss} />} note={d.covered_loss > 0 ? <>qoplangan: {som(d.covered_loss)}</> : undefined} tone={d.defaulted_loss > 0 ? 'bad' : undefined} />
        <Tile label="Muzlatilgan" value={<Money v={d.frozen} />} tone={d.frozen > 0 ? 'bad' : undefined} />
      </Tiles>

      <Card title="Pozitsiyalar (segregated: aniq shartnomalardagi ulush)">
        {d.positions.length === 0 ? (
          <Empty>Kapital hali shartnomalarga taqsimlanmagan</Empty>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Tovar</th><th>Holat</th><th>Ulush</th><th>Kiritilgan</th><th>Qaytgan</th><th>Qoldiq</th><th>Zarar</th><th>Kutilayotgan foyda</th>
              </tr>
            </thead>
            <tbody>
              {d.positions.map((p) => (
                <tr key={p.contract_id}>
                  <td>{p.goods}</td>
                  <td><StatusBadge s={p.status} /></td>
                  <td>{bps(p.share_bps)}</td>
                  <td><Money v={p.principal} /></td>
                  <td><Money v={p.repaid} /></td>
                  <td><Money v={p.outstanding} /></td>
                  <td>{p.written_off > 0 ? <span className="bad-text"><Money v={p.written_off} /></span> : '—'}</td>
                  <td><Money v={p.expected_profit} /> <small className="muted">({p.expected_profit_label})</small></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <Card title="Dividendlar va ularning kelib chiqishi (lineage)">
        {d.dividends.length === 0 ? (
          <Empty>Dividend hali to'lanmagan</Empty>
        ) : (
          <div className="list">
            {d.dividends.map((v) => (
              <div key={v.payment_id} className="list-item">
                <div className="row-between">
                  <div>
                    <strong><Money v={v.amount} /></strong> · {dt(v.at)} <PaymentBadge s={v.status} />
                  </div>
                  <button className="link" onClick={() => setOpen(open === v.payment_id ? null : v.payment_id)}>
                    {open === v.payment_id ? 'Yopish' : "Qayerdan keldi?"}
                  </button>
                </div>
                <ul className="sources">
                  {v.sources.map((s, i) => (
                    <li key={i}>
                      <Money v={s.amount} /> — <strong>{s.goods ?? '?'}</strong> shartnomasining {s.month ?? '?'} oydagi ustamasidan
                      {open === v.payment_id && <LineageView steps={s.lineage} />}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}
      </Card>

      <Withdraw id={id} liquid={d.liquid} onDone={reload} />
    </div>
  )
}

function Withdraw({ id, liquid, onDone }: { id: string; liquid: number; onDone: () => void }) {
  const [amount, setAmount] = useState('')
  const [plan, setPlan] = useState<WithdrawalPlan | null>(null)
  const a = useAction()
  const t = toTiyin(amount)
  async function preview() {
    const p = await a.run(() => api.get<WithdrawalPlan>(`/api/withdrawals/preview?investor=${id}&amount=${t}`), () => null)
    if (p) setPlan(p)
  }
  async function submit() {
    const r = await a.run(() => api.post<{ plan: WithdrawalPlan }>('/api/withdrawals', { investor: id, amount: t }))
    if (r) {
      setPlan(r.plan)
      onDone()
    }
  }
  return (
    <Card title="Kapitalni yechib olish">
      <p className="muted">
        Faqat o'z likvid kapitalingizdan to'lanadi ({som(liquid)}). Qolgani shartnomalar to'lov jadvali bo'yicha bo'shaydi —
        boshqa investorning pulidan to'lab bo'lmaydi (I2).
      </p>
      <div className="form-row">
        <Field label="Summa (so'm)">
          <input value={amount} onChange={(e) => setAmount(e.target.value)} placeholder="3 000 000" />
        </Field>
        <button disabled={!(t > 0) || a.busy} onClick={preview}>Hisoblash</button>
        <button disabled={!(t > 0) || a.busy} className="primary" onClick={submit}>Yechib olish</button>
      </div>
      <ActionResult a={a} />
      {plan && (
        <div className="plan">
          <p>
            So'ralgan <Money v={plan.requested} />: hozir <strong><Money v={plan.payable_now} /></strong>
            {plan.remaining > 0 && <>, qolgan <strong><Money v={plan.remaining} /></strong> quyidagi jadval bo'yicha bo'shaydi (taxminiy, kafolatlanmagan):</>}
          </p>
          {plan.schedule.length > 0 && (
            <table>
              <thead><tr><th>Sana</th><th>Bo'shaydigan</th><th>Jami</th></tr></thead>
              <tbody>
                {plan.schedule.map((r) => (
                  <tr key={r.date}><td>{day(r.date)}</td><td><Money v={r.amount} /></td><td><Money v={r.cumulative} /></td></tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </Card>
  )
}
