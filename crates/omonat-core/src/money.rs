//! Pul summasi: butun son, tiyinda (1 so'm = 100 tiyin). Float hech qayerda ishlatilmaydi.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(pub i64);

impl Money {
    pub const ZERO: Money = Money(0);

    /// Butun so'mdan.
    pub const fn som(n: i64) -> Money {
        Money(n * 100)
    }

    pub const fn tiyin(self) -> i64 {
        self.0
    }

    pub fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// `self * bps / 10_000`, pastga yaxlitlangan.
    pub fn mul_bps_floor(self, bps: u32) -> Money {
        Money(((self.0 as i128 * bps as i128) / 10_000) as i64)
    }

    /// `self * num / den`, pastga yaxlitlangan (den > 0).
    pub fn mul_div_floor(self, num: Money, den: Money) -> Money {
        assert!(den.0 > 0, "mul_div_floor: den must be positive");
        Money(((self.0 as i128 * num.0 as i128).div_euclid(den.0 as i128)) as i64)
    }

    pub fn min(self, other: Money) -> Money {
        if self <= other { self } else { other }
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(self.0.checked_add(rhs.0).expect("money overflow"))
    }
}
impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money(self.0.checked_sub(rhs.0).expect("money overflow"))
    }
}
impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}
impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Money) {
        *self = *self + rhs;
    }
}
impl SubAssign for Money {
    fn sub_assign(&mut self, rhs: Money) {
        *self = *self - rhs;
    }
}
impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money::ZERO, |a, b| a + b)
    }
}
impl<'a> Sum<&'a Money> for Money {
    fn sum<I: Iterator<Item = &'a Money>>(iter: I) -> Money {
        iter.copied().sum()
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        write!(f, "{sign}{}.{:02} so'm", abs / 100, abs % 100)
    }
}

/// `total` ni `weights` ga mutanosib taqsimlaydi (largest remainder / Hamilton usuli).
///
/// Natija yig'indisi har doim aynan `total` ga teng. Har bir ulush `weight * total / W` ning
/// pastki yoki yuqori butun qismi. Qoldiq tiyinlar eng katta kasr qismiga, teng bo'lsa kalit
/// tartibida beriladi — natija deterministik. `total <= Σweights` bo'lsa, hech bir ulush o'z
/// vaznidan oshmaydi.
pub fn split_pro_rata<K: Ord + Clone>(total: Money, weights: &[(K, Money)]) -> Vec<(K, Money)> {
    let w_sum: i128 = weights.iter().map(|(_, w)| w.0.max(0) as i128).sum();
    if weights.is_empty() || w_sum == 0 || total.0 == 0 {
        return weights.iter().map(|(k, _)| (k.clone(), Money::ZERO)).collect();
    }
    assert!(total.0 > 0, "split_pro_rata: negative total");
    let t = total.0 as i128;
    let mut parts: Vec<(usize, i64, i128)> = weights
        .iter()
        .enumerate()
        .map(|(i, (_, w))| {
            let num = t * (w.0.max(0) as i128);
            ((i), (num / w_sum) as i64, num % w_sum)
        })
        .collect();
    let assigned: i64 = parts.iter().map(|p| p.1).sum();
    let mut leftover = total.0 - assigned;
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by(|&a, &b| {
        parts[b]
            .2
            .cmp(&parts[a].2)
            .then_with(|| weights[parts[a].0].0.cmp(&weights[parts[b].0].0))
    });
    for idx in order {
        if leftover == 0 {
            break;
        }
        if parts[idx].2 > 0 {
            parts[idx].1 += 1;
            leftover -= 1;
        }
    }
    debug_assert_eq!(leftover, 0);
    parts
        .into_iter()
        .map(|(i, amt, _)| (weights[i].0.clone(), Money(amt)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pro_rata_sums_exactly() {
        let w = vec![("a", Money(1)), ("b", Money(1)), ("c", Money(1))];
        let r = split_pro_rata(Money(100), &w);
        assert_eq!(r.iter().map(|x| x.1).sum::<Money>(), Money(100));
        assert_eq!(r, vec![("a", Money(34)), ("b", Money(33)), ("c", Money(33))]);
    }

    #[test]
    fn pro_rata_never_exceeds_weight_when_total_le_sum() {
        let w = vec![(1, Money(7)), (2, Money(13)), (3, Money(1))];
        for t in 0..=21 {
            let r = split_pro_rata(Money(t), &w);
            assert_eq!(r.iter().map(|x| x.1).sum::<Money>(), Money(t));
            for ((_, got), (_, wt)) in r.iter().zip(w.iter()) {
                assert!(got <= wt);
            }
        }
    }
}
