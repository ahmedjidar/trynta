//! Just enough big-integer arithmetic to count a character space exactly.
//!
//! SPEC-V1 §7.3 says the generator's entropy must be computed by
//! inclusion–exclusion and offers two ways to do it: *"Use exact big integers or
//! f64 with care."* This is the exact-integer route, and it is here rather than
//! in a dependency for two reasons.
//!
//! The first is that AC12 asks the number to match an independent
//! inclusion–exclusion implementation **exactly**. `floor(log2(n))` computed in
//! f64 is off by one whenever the true value lands within a few ulps of an
//! integer, and "usually right" is not what that criterion asks for.
//!
//! The second is proportion. The whole requirement is: raise a small integer to
//! a power of at most 128, add and subtract at most sixteen such terms, and take
//! a bit length. That is four operations on an unsigned magnitude — not a reason
//! to add a general-purpose numeric crate to a process that holds key material
//! (CLAUDE.md §2).
//!
//! This module does arithmetic, not cryptography. Nothing here is secret,
//! nothing here is timing-sensitive, and no key material ever reaches it: the
//! inputs are charset sizes and a length, both of which the user chose and both
//! of which are already on screen.

use std::cmp::Ordering;

/// A non-negative integer, little-endian in base 2³².
///
/// Deliberately minimal: no division, no formatting, no signed form. Terms are
/// accumulated into separate positive and negative sums so a subtraction never
/// has to go below zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Big(Vec<u32>);

impl Big {
    /// Zero.
    #[must_use]
    pub const fn zero() -> Self {
        Self(Vec::new())
    }

    /// One.
    #[must_use]
    pub fn one() -> Self {
        Self(vec![1])
    }

    /// `base` raised to `exp`.
    ///
    /// Repeated multiplication rather than square-and-multiply: `exp` is a
    /// password length, so at most 128, and the obvious loop is easier to be
    /// sure about than the clever one.
    #[must_use]
    pub fn pow(base: u32, exp: u32) -> Self {
        if base == 0 {
            return if exp == 0 { Self::one() } else { Self::zero() };
        }
        let mut out = Self::one();
        for _ in 0..exp {
            out.mul_u32(base);
        }
        out
    }

    /// Whether this is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&limb| limb == 0)
    }

    /// Multiply in place by a small factor.
    pub fn mul_u32(&mut self, m: u32) {
        if m == 0 || self.is_zero() {
            self.0.clear();
            return;
        }
        let mut carry: u64 = 0;
        for limb in &mut self.0 {
            let product = u64::from(*limb) * u64::from(m) + carry;
            *limb = u32::try_from(product & 0xFFFF_FFFF).unwrap_or(0);
            carry = product >> 32;
        }
        while carry > 0 {
            self.0.push(u32::try_from(carry & 0xFFFF_FFFF).unwrap_or(0));
            carry >>= 32;
        }
    }

    /// Add `other` in place.
    pub fn add_assign(&mut self, other: &Self) {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        let mut carry: u64 = 0;
        for (i, limb) in self.0.iter_mut().enumerate() {
            let rhs = other.0.get(i).copied().unwrap_or(0);
            let sum = u64::from(*limb) + u64::from(rhs) + carry;
            *limb = u32::try_from(sum & 0xFFFF_FFFF).unwrap_or(0);
            carry = sum >> 32;
        }
        if carry > 0 {
            self.0.push(u32::try_from(carry).unwrap_or(0));
        }
    }

    /// Subtract `other` in place.
    ///
    /// # Panics
    ///
    /// Never in this crate's use: callers sum the positive and negative
    /// inclusion–exclusion terms separately and the identity guarantees the
    /// positive sum is the larger. The assertion is kept so a future caller that
    /// breaks that invariant fails loudly rather than silently wrapping into a
    /// nonsense entropy figure.
    pub fn sub_assign(&mut self, other: &Self) {
        assert!(
            self.cmp_big(other) != Ordering::Less,
            "big-integer subtraction would go negative"
        );
        let mut borrow: i64 = 0;
        for (i, limb) in self.0.iter_mut().enumerate() {
            let rhs = i64::from(other.0.get(i).copied().unwrap_or(0));
            let mut diff = i64::from(*limb) - rhs - borrow;
            if diff < 0 {
                diff += 1 << 32;
                borrow = 1;
            } else {
                borrow = 0;
            }
            *limb = u32::try_from(diff).unwrap_or(0);
        }
        self.trim();
    }

    /// Compare magnitudes.
    #[must_use]
    pub fn cmp_big(&self, other: &Self) -> Ordering {
        let a = self.significant();
        let b = other.significant();
        if a != b {
            return a.cmp(&b);
        }
        for i in (0..a).rev() {
            let l = self.0.get(i).copied().unwrap_or(0);
            let r = other.0.get(i).copied().unwrap_or(0);
            if l != r {
                return l.cmp(&r);
            }
        }
        Ordering::Equal
    }

    /// Number of significant bits. Zero has none.
    #[must_use]
    pub fn bits(&self) -> u32 {
        let significant = self.significant();
        if significant == 0 {
            return 0;
        }
        let top = self.0.get(significant - 1).copied().unwrap_or(0);
        u32::try_from(significant - 1).unwrap_or(0) * 32 + (32 - top.leading_zeros())
    }

    /// `floor(log2(self))`, or `None` for zero.
    #[must_use]
    pub fn floor_log2(&self) -> Option<u32> {
        self.bits().checked_sub(1)
    }

    fn significant(&self) -> usize {
        let mut n = self.0.len();
        while n > 0 && self.0.get(n - 1).copied().unwrap_or(0) == 0 {
            n -= 1;
        }
        n
    }

    fn trim(&mut self) {
        let n = self.significant();
        self.0.truncate(n);
    }
}

#[cfg(test)]
mod tests {
    use super::Big;

    #[test]
    fn powers_of_two_have_the_bit_length_they_should() {
        for exp in 0..200u32 {
            assert_eq!(
                Big::pow(2, exp).floor_log2(),
                Some(exp),
                "2^{exp} reported the wrong bit length"
            );
        }
    }

    #[test]
    fn zero_and_one_are_handled() {
        assert!(Big::zero().is_zero());
        assert_eq!(Big::zero().floor_log2(), None);
        assert_eq!(Big::one().floor_log2(), Some(0));
        assert!(Big::pow(0, 5).is_zero());
        assert_eq!(Big::pow(0, 0).floor_log2(), Some(0));
        assert_eq!(Big::pow(7, 0).floor_log2(), Some(0));
    }

    #[test]
    fn small_powers_match_u128() {
        for base in [2u32, 3, 10, 26, 62, 95] {
            for exp in 0..=12u32 {
                let expected = u128::from(base).pow(exp);
                let got = Big::pow(base, exp);
                assert_eq!(got.floor_log2(), Some(expected.ilog2()), "{base}^{exp}");
            }
        }
    }

    #[test]
    fn addition_and_subtraction_round_trip() {
        let mut a = Big::pow(95, 40);
        let b = Big::pow(94, 40);
        let original = a.clone();
        a.add_assign(&b);
        a.sub_assign(&b);
        assert_eq!(a, original);
    }

    #[test]
    fn subtraction_borrows_across_limbs() {
        // 2^64 - 1 exercises a borrow through every limb.
        let mut a = Big::pow(2, 64);
        a.sub_assign(&Big::one());
        assert_eq!(a.floor_log2(), Some(63));
    }

    #[test]
    #[should_panic(expected = "would go negative")]
    fn subtracting_more_than_there_is_panics_rather_than_wrapping() {
        let mut a = Big::one();
        a.sub_assign(&Big::pow(2, 10));
    }
}
