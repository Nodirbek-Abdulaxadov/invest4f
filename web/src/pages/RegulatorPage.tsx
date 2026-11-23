import type { RegulatorView } from '../api'
import { useApi } from '../api'
import { AnchorsTable, IncidentsList, VerifyPanel } from '../components/shared'
import { Badge, Card, Empty, ErrorText, Money, Tile, Tiles } from '../components/ui'
import { bps, dt, statusLabel } from '../format'
import type { ContractStatus } from '../api'

export default function RegulatorPage() {
  const { data: v, error } = useApi<RegulatorView>('/api/regulator')
  if (error) return <ErrorText e={error} />
  if (!v) return <div className="muted">Yuklanmoqda…</div>
  return (
    <div className="page">
      <h1>Regulyator ko'rinishi</h1>
      <p className="muted">Read-only agregat ko'rsatkichlar. Investorlarning shaxsiy ma'lumotlari ko'rsatilmaydi.</p>
      <Tiles>
        <Tile label="AUM (investorlar kapitali)" value={<Money v={v.aum} />} note={`${v.investors} ta investor`} />
        <Tile label="Likvidlik" value={<Money v={v.liquidity} />} note={bps(v.liquidity_bps)} />
        <Tile label="Faol portfel" value={<Money v={v.outstanding} />} />
        <Tile label="NPL (muddati o'tgan)" value={<Money v={v.npl} />} note={bps(v.npl_bps)} tone={v.npl_bps > 1000 ? 'bad' : v.npl > 0 ? 'warn' : undefined} />
        <Tile label="Hisobdan chiqarilgan zarar" value={<Money v={v.written_off} />} tone={v.written_off > 0 ? 'bad' : undefined} />
        <Tile label="Rezerv" value={<Money v={v.reserve} />} />
        <Tile label="Xayriya hisobi" value={<Money v={v.charity} />} />
        <Tile label="Rad etilgan urinishlar" value={v.rejected_count} tone={v.rejected_count > 0 ? 'warn' : undefined} />
      </Tiles>

      <div className="two-col">
        <Card title="Shartnomalar holati">
          <table>
            <tbody>
              {Object.entries(v.contracts_by_status).map(([k, n]) => (
                <tr key={k}><td>{statusLabel[k as ContractStatus] ?? k}</td><td>{n}</td></tr>
              ))}
            </tbody>
          </table>
        </Card>
        <Card title="Incident'lar"><IncidentsList list={v.incidents} /></Card>
      </div>

      <Card title="Reconciliation natijalari (ledger ↔ bank, I11)">
        {v.reconciliations.length === 0 ? <Empty /> : (
          <table>
            <thead><tr><th>Vaqt</th><th>Ledger</th><th>Bank</th><th>Natija</th></tr></thead>
            <tbody>
              {v.reconciliations.map((r) => (
                <tr key={r.id}>
                  <td className="muted">{dt(r.at)}</td>
                  <td><Money v={r.ledger_balance} /></td>
                  <td><Money v={r.bank_balance} /></td>
                  <td>{r.ok ? <Badge tone="good">mos</Badge> : <Badge tone="bad">farq: <Money v={r.bank_balance - r.ledger_balance} /></Badge>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <VerifyPanel />
      <Card title="Kunlik Merkle root'lar"><AnchorsTable anchors={v.anchors} /></Card>
    </div>
  )
}
