import type { ContractStatus, MoneyClass, PaymentStatus, TxKind } from './api'

/** Tiyin → "1 234 567,89 so'm". */
export function som(tiyin: number | null | undefined, withUnit = true): string {
  const t = tiyin ?? 0
  const sign = t < 0 ? '-' : ''
  const abs = Math.abs(t)
  const whole = Math.floor(abs / 100)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, ' ')
  const frac = (abs % 100).toString().padStart(2, '0')
  return `${sign}${whole},${frac}${withUnit ? ' so‘m' : ''}`
}

/** "1 200 000,50" yoki "1200000.5" → tiyin. Noto'g'ri bo'lsa NaN. */
export function toTiyin(input: string): number {
  const s = input.replace(/[\s ]/g, '').replace(',', '.')
  if (!/^\d+(\.\d{0,2})?$/.test(s)) return NaN
  const [w, f = ''] = s.split('.')
  return Number(w) * 100 + Number(f.padEnd(2, '0'))
}

export function bps(n: number): string {
  return `${(n / 100).toFixed(2)}%`
}

const dtf = new Intl.DateTimeFormat('ru-RU', {
  timeZone: 'Asia/Tashkent',
  day: '2-digit',
  month: '2-digit',
  year: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
})

export function dt(iso: string | null | undefined): string {
  return iso ? dtf.format(new Date(iso)) : '—'
}

export function day(iso: string): string {
  const [y, m, d] = iso.split('-')
  return `${d}.${m}.${y}`
}

export function short(id: string | null | undefined, n = 8): string {
  return id ? id.slice(0, n) : '—'
}

export const classLabel: Record<MoneyClass, string> = {
  Capital: 'Kapital',
  Profit: 'Shartnoma foydasi',
  InvestorProfit: 'Investor foydasi',
  MudaribShare: 'Mudarib ulushi',
  Reserve: 'Rezerv',
  Charity: 'Xayriya (jarima)',
}

export const statusLabel: Record<ContractStatus, string> = {
  Draft: 'Qoralama',
  GoodsPurchased: 'Tovar xarid qilingan',
  Active: 'Faol',
  Repaid: "To'liq to'langan",
  Overdue: "Muddati o'tgan",
  Defaulted: 'Defolt',
}

export const kindLabel: Record<TxKind, string> = {
  Deposit: 'Depozit',
  Deploy: 'Tovar xaridi',
  Installment: "Mijoz to'lovi",
  ProfitSplit: 'Foyda taqsimoti',
  Dividend: 'Dividend',
  Withdrawal: 'Yechib olish',
  Expense: 'Xarajat',
  ReserveUse: 'Rezervdan qoplash',
  LossAllocation: 'Zarar taqsimoti',
  Donation: 'Xayriya',
  Compensation: 'Kompensatsiya',
}

export const paymentLabel: Record<PaymentStatus, string> = {
  Pending: 'Bankda',
  Confirmed: 'Bajarildi',
  Failed: 'Bank rad etdi',
}

export const ruleLabel: Record<string, string> = {
  STRUCT: 'Ledger tuzilmasi',
  FLOW: 'Ruxsat etilmagan oqim',
  I1: 'Dividend faqat realized ustamadan',
  I2: 'Egalar lotlari aralashmaydi',
  I3: "Xarajat faqat mudarib ulushidan",
  I4: 'Sotishdan oldin egalik',
  I5: 'Rezerv cap',
  I6: 'Mudarib ulushi',
  I7: 'Kechikish jarimasi — xayriyaga',
  I8: 'Related party — 2-of-2',
  I9: 'Rezerv — Sharia board qarori',
  I10: 'Pro-rata taqsimot',
  I11: 'Reconciliation',
  I12: 'Append-only / hash chain',
  I13: "Kafolatlangan daromad yo'q",
}
