import { useCallback, useEffect, useState } from 'react'
import { NavLink, Navigate, Route, Routes, useNavigate } from 'react-router-dom'
import type { Health, Meta, Party, Session } from './api'
import { api, getSession, investorId, setSession, useApi } from './api'
import { MetaContext } from './components/shared'
import { Badge, ErrorText } from './components/ui'
import { day } from './format'
import AdminPage from './pages/AdminPage'
import AuditorPage from './pages/AuditorPage'
import InvestorPage from './pages/InvestorPage'
import OperatorPage from './pages/OperatorPage'
import RegulatorPage from './pages/RegulatorPage'

type RoleKey = 'operator' | 'sharia' | 'regulator' | 'admin' | 'investor'

function roleOf(s: Session): RoleKey {
  return s.actor.startsWith('investor:') ? 'investor' : (s.actor as RoleKey)
}

const NAV: Record<RoleKey, { to: string; label: string }[]> = {
  investor: [{ to: '/investor', label: 'Mening hisobim' }],
  operator: [{ to: '/operator', label: 'Operator' }],
  sharia: [
    { to: '/auditor', label: 'Sharia auditor' },
    { to: '/regulator', label: 'Agregatlar' },
  ],
  regulator: [{ to: '/regulator', label: 'Regulyator' }],
  admin: [
    { to: '/admin', label: 'Dev / bank simulyatori' },
    { to: '/auditor', label: 'Auditor' },
    { to: '/regulator', label: 'Regulyator' },
  ],
}

function Login({ onLogin }: { onLogin: (s: Session) => void }) {
  const [investors, setInvestors] = useState<Party[]>([])
  const [inv, setInv] = useState('')
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    // Dev kirish: ro'yxatni admin sifatida o'qiymiz (production'da OIDC).
    api
      .get<Meta>('/api/meta', 'admin')
      .then((m) => setInvestors(m.parties.filter((p) => p.kind === 'Investor')))
      .catch((e) => setError((e as Error).message))
  }, [])
  const roles: { actor: string; label: string; desc: string }[] = [
    { actor: 'operator', label: 'Operator (mudarib)', desc: "Shartnomalar, to'lov takliflari. Yolg'iz to'lov qila olmaydi." },
    { actor: 'sharia', label: 'Sharia board', desc: "Policy imzosi, 2-of-2 tasdiq, real-time audit." },
    { actor: 'regulator', label: 'Regulyator', desc: "Read-only agregatlar, incident'lar." },
    { actor: 'admin', label: 'Admin / dev', desc: "Bank simulyatori, reconciliation, freeze, seed." },
  ]
  return (
    <div className="login">
      <h1>Omonat</h1>
      <p className="lead">Shariatga mos investitsiya fondlari uchun enforcement ledger. Detection emas — enforcement.</p>
      <ErrorText e={error} />
      <div className="login-grid">
        <div className="login-card">
          <h3>Investor (rabb al-mal)</h3>
          <p className="muted">O'z lotlari, pozitsiyalari, foyda lineage'i.</p>
          <select value={inv} onChange={(e) => setInv(e.target.value)}>
            <option value="">— investorni tanlang —</option>
            {investors.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
          <button disabled={!inv} onClick={() => onLogin({ actor: `investor:${inv}`, label: investors.find((p) => p.id === inv)?.name ?? 'Investor' })}>
            Kirish
          </button>
          {investors.length === 0 && <small className="muted">Investor yo'q — admin sifatida kirib, demo ma'lumotlarni yarating.</small>}
        </div>
        {roles.map((r) => (
          <div key={r.actor} className="login-card">
            <h3>{r.label}</h3>
            <p className="muted">{r.desc}</p>
            <button onClick={() => onLogin({ actor: r.actor, label: r.label })}>Kirish</button>
          </div>
        ))}
      </div>
      <p className="muted small">Dev rejimi: rol `X-Omonat-Actor` sarlavhasi bilan yuboriladi. Production'da OIDC va har bir rolning o'z kaliti.</p>
    </div>
  )
}

export default function App() {
  const [session, setS] = useState<Session | null>(getSession())
  const [meta, setMeta] = useState<Meta | null>(null)
  const navigate = useNavigate()
  const health = useApi<Health>('/api/health', 5000)

  const loadMeta = useCallback(() => {
    if (!session) return
    api.get<Meta>('/api/meta').then(setMeta).catch(() => setMeta(null))
  }, [session])
  useEffect(loadMeta, [loadMeta])

  function login(s: Session) {
    setSession(s)
    setS(s)
    navigate(NAV[roleOf(s)][0].to)
  }
  function logout() {
    setSession(null)
    setS(null)
    setMeta(null)
    navigate('/')
  }

  if (!session) return <Login onLogin={login} />
  const role = roleOf(session)
  const h = health.data
  return (
    <MetaContext.Provider value={{ meta, reload: loadMeta }}>
      <header className="top">
        <div className="brand">Omonat</div>
        <nav>
          {NAV[role].map((n) => (
            <NavLink key={n.to} to={n.to}>{n.label}</NavLink>
          ))}
        </nav>
        <div className="top-right">
          {h && (
            <>
              {h.integrity_error ? <Badge tone="bad" title={h.integrity_error}>Ledger buzilgan — yozish to'xtatilgan</Badge> : <Badge tone="good">Ledger butun · #{h.head_seq}</Badge>}
              <span className="muted">{day(h.today)}{h.clock_offset_days > 0 && ` (+${h.clock_offset_days} kun)`}</span>
              <Badge tone="muted">{h.store}</Badge>
            </>
          )}
          <span className="who">{session.label}</span>
          <button className="ghost" onClick={logout}>Chiqish</button>
        </div>
      </header>
      {h?.integrity_error && <div className="banner bad">INCIDENT (I12): {h.integrity_error}</div>}
      <main>
        <Routes>
          <Route path="/investor" element={investorId(session) ? <InvestorPage id={investorId(session)!} /> : <Navigate to="/" />} />
          <Route path="/operator" element={<OperatorPage />} />
          <Route path="/auditor" element={<AuditorPage />} />
          <Route path="/regulator" element={<RegulatorPage />} />
          <Route path="/admin" element={<AdminPage />} />
          <Route path="*" element={<Navigate to={NAV[role][0].to} />} />
        </Routes>
      </main>
    </MetaContext.Provider>
  )
}
