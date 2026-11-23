//! Murabaha shartnomasi hisob-kitoblari: to'lov jadvali va to'lovni asosiy/ustamaga bo'lish.

use crate::model::{Contract, Installment};
use crate::money::Money;
use chrono::{Months, NaiveDate};

/// `n` oylik teng to'lov jadvali. Yaxlitlash qoldig'i oxirgi to'lovga qo'shiladi.
pub fn build_schedule(cost: Money, markup: Money, n: u32, first_due: NaiveDate) -> Vec<Installment> {
    assert!(n > 0);
    let p_each = Money(cost.0 / n as i64);
    let m_each = Money(markup.0 / n as i64);
    (0..n)
        .map(|i| {
            let last = i + 1 == n;
            Installment {
                no: i + 1,
                due: first_due.checked_add_months(Months::new(i)).expect("valid date"),
                principal: if last { cost - Money(p_each.0 * (n as i64 - 1)) } else { p_each },
                markup: if last { markup - Money(m_each.0 * (n as i64 - 1)) } else { m_each },
            }
        })
        .collect()
}

/// To'lov (jarimasiz) qismini asosiy qism va ustamaga bo'ladi.
///
/// Ustama kumulyativ hisoblanadi: to'langan jami × ustama / narx (pastga yaxlitlab), shuning uchun
/// yaxlitlash xatosi to'planmaydi va oxirgi to'lovda ikkalasi ham aniq yopiladi.
/// `amount` qolgan narxdan oshmasligi kerak.
pub fn split_payment(c: &Contract, amount: Money) -> (Money, Money) {
    let price = c.price();
    let after = c.paid_total() + amount;
    let markup_cum = if after >= price { c.markup } else { after.mul_div_floor(c.markup, price) };
    let markup_part = (markup_cum - c.paid_markup).min(c.remaining_markup());
    let markup_part = if markup_part.0 < 0 { Money::ZERO } else { markup_part };
    (amount - markup_part, markup_part)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ContractStatus;
    use chrono::Utc;
    use uuid::Uuid;

    fn contract(cost: i64, markup: i64) -> Contract {
        Contract {
            id: Uuid::new_v4(),
            merchant: Uuid::new_v4(),
            customer: Uuid::new_v4(),
            goods: "x".into(),
            cost: Money(cost),
            markup: Money(markup),
            schedule: vec![],
            status: ContractStatus::Active,
            ownership_doc: None,
            purchase_tx: None,
            paid_principal: Money::ZERO,
            paid_markup: Money::ZERO,
            late_fees: Money::ZERO,
            fault: None,
            mudarib_liability: Money::ZERO,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn schedule_sums_exactly() {
        let s = build_schedule(Money(1000), Money(101), 3, NaiveDate::from_ymd_opt(2026, 1, 31).unwrap());
        assert_eq!(s.iter().map(|i| i.principal).sum::<Money>(), Money(1000));
        assert_eq!(s.iter().map(|i| i.markup).sum::<Money>(), Money(101));
        assert_eq!(s[1].due, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
    }

    #[test]
    fn split_closes_exactly() {
        let mut c = contract(1000, 333);
        for pay in [400, 400, 533] {
            let (p, m) = split_payment(&c, Money(pay));
            c.paid_principal += p;
            c.paid_markup += m;
        }
        assert_eq!(c.paid_principal, Money(1000));
        assert_eq!(c.paid_markup, Money(333));
    }
}
