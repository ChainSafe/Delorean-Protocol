// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
// to silence construct_uint! clippy warnings
// see https://github.com/paritytech/parity-common/issues/660
#![allow(clippy::ptr_offset_with_cast, clippy::assign_op_pattern)]

use serde::{Deserialize, Serialize};
//use substrate_bn::arith;

use {
    fvm_shared::bigint::BigInt, fvm_shared::econ::TokenAmount, std::cmp::Ordering, std::fmt,
    uint::construct_uint,
};

construct_uint! { pub struct U256(4); } // ethereum word size
construct_uint! { pub struct U512(8); } // used for addmod and mulmod opcodes

// Convenience method for comparing against a small value.
impl PartialOrd<u64> for U256 {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        if self.0[3] > 0 || self.0[2] > 0 || self.0[1] > 0 {
            Some(Ordering::Greater)
        } else {
            self.0[0].partial_cmp(other)
        }
    }
}

impl PartialEq<u64> for U256 {
    fn eq(&self, other: &u64) -> bool {
        self.0[0] == *other && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }
}

impl U256 {
    pub const BITS: u32 = 256;
    pub const ZERO: Self = U256::from_u64(0);
    pub const ONE: Self = U256::from_u64(1);
    pub const I128_MIN: Self = U256([0, 0, 0, i64::MIN as u64]);

    #[inline(always)]
    pub const fn from_u128_words(high: u128, low: u128) -> U256 {
        U256([
            low as u64,
            (low >> u64::BITS) as u64,
            high as u64,
            (high >> u64::BITS) as u64,
        ])
    }

    #[inline(always)]
    pub const fn from_u64(value: u64) -> U256 {
        U256([value, 0, 0, 0])
    }

    #[inline(always)]
    pub const fn i256_is_negative(&self) -> bool {
        (self.0[3] as i64) < 0
    }

    /// turns a i256 value to negative
    #[inline(always)]
    pub fn i256_neg(&self) -> U256 {
        if self.is_zero() {
            U256::ZERO
        } else {
            !*self + U256::ONE
        }
    }

    #[inline(always)]
    pub fn i256_cmp(&self, other: &U256) -> Ordering {
        // true > false:
        // - true < positive:
        match other.i256_is_negative().cmp(&self.i256_is_negative()) {
            Ordering::Equal => self.cmp(other),
            sign_cmp => sign_cmp,
        }
    }

    #[inline]
    pub fn i256_div(&self, other: &U256) -> U256 {
        if self.is_zero() || other.is_zero() {
            // EVM defines X/0 to be 0.
            return U256::ZERO;
        }

        let mut first = *self;
        let mut second = *other;

        // Record and strip the signs. We add them back at the end.
        let first_neg = first.i256_is_negative();
        let second_neg = second.i256_is_negative();

        if first_neg {
            first = first.i256_neg()
        }

        if second_neg {
            second = second.i256_neg()
        }

        let d = first / second;

        // Flip the sign back if necessary.
        if d.is_zero() || first_neg == second_neg {
            d
        } else {
            d.i256_neg()
        }
    }

    #[inline]
    pub fn i256_mod(&self, other: &U256) -> U256 {
        if self.is_zero() || other.is_zero() {
            // X % 0  or 0 % X is always 0.
            return U256::ZERO;
        }

        let mut first = *self;
        let mut second = *other;

        // Record and strip the sign.
        let negative = first.i256_is_negative();
        if negative {
            first = first.i256_neg();
        }

        if second.i256_is_negative() {
            second = second.i256_neg()
        }

        let r = first % second;

        // Restore the sign.
        if negative && !r.is_zero() {
            r.i256_neg()
        } else {
            r
        }
    }

    pub fn to_bytes(self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        self.to_big_endian(&mut buf);
        buf
    }

    /// Returns the low 64 bits, saturating the value to u64 max if it is larger
    pub fn to_u64_saturating(self) -> u64 {
        if self.bits() > 64 {
            u64::MAX
        } else {
            self.0[0]
        }
    }
}

impl U512 {
    pub fn low_u256(&self) -> U256 {
        let [a, b, c, d, ..] = self.0;
        U256([a, b, c, d])
    }
}

impl From<&TokenAmount> for U256 {
    fn from(amount: &TokenAmount) -> U256 {
        let (_, bytes) = amount.atto().to_bytes_be();
        U256::from(bytes.as_slice())
    }
}

impl From<U256> for U512 {
    fn from(v: U256) -> Self {
        let [a, b, c, d] = v.0;
        U512([a, b, c, d, 0, 0, 0, 0])
    }
}

impl From<&U256> for TokenAmount {
    fn from(ui: &U256) -> TokenAmount {
        let mut bits = [0u8; 32];
        ui.to_big_endian(&mut bits);
        TokenAmount::from_atto(BigInt::from_bytes_be(fvm_shared::bigint::Sign::Plus, &bits))
    }
}

impl Serialize for U256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut bytes = [0u8; 32];
        self.to_big_endian(&mut bytes);
        serializer.serialize_bytes(zeroless_view(&bytes))
    }
}

impl<'de> Deserialize<'de> for U256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = U256;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "at most 32 bytes")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v.len() > 32 {
                    return Err(serde::de::Error::invalid_length(v.len(), &self));
                }
                Ok(U256::from_big_endian(v))
            }
        }
        deserializer.deserialize_bytes(Visitor)
    }
}

fn zeroless_view(v: &impl AsRef<[u8]>) -> &[u8] {
    let v = v.as_ref();
    &v[v.iter().take_while(|&&b| b == 0).count()..]
}

#[cfg(test)]
mod tests {
    use fvm_ipld_encoding::{BytesDe, BytesSer, RawBytes};

    use {super::*, core::num::Wrapping};

    #[test]
    fn div_i256() {
        assert_eq!(Wrapping(i8::MIN) / Wrapping(-1), Wrapping(i8::MIN));
        assert_eq!(i8::MAX / -1, -i8::MAX);

        let zero = U256::ZERO;
        let one = U256::ONE;
        let one_hundred = U256::from(100);
        let fifty = U256::from(50);
        let two = U256::from(2);
        let neg_one_hundred = U256::from(100);
        let minus_one = U256::from(1);
        let max_value = U256::from(2).pow(255.into()) - 1;
        let neg_max_value = U256::from(2).pow(255.into()) - 1;

        assert_eq!(U256::I128_MIN.i256_div(&minus_one), U256::I128_MIN);
        assert_eq!(U256::I128_MIN.i256_div(&one), U256::I128_MIN);
        assert_eq!(
            U256::I128_MIN.i256_div(&two),
            U256([0, 0, 0, i64::MIN as u64 + (i64::MIN as u64 >> 1)])
        );
        assert_eq!(one.i256_div(&U256::I128_MIN), zero);
        assert_eq!(max_value.i256_div(&one), max_value);
        assert_eq!(max_value.i256_div(&minus_one), neg_max_value);
        assert_eq!(one_hundred.i256_div(&minus_one), neg_one_hundred);
        assert_eq!(one_hundred.i256_div(&two), fifty);

        assert_eq!(zero.i256_div(&zero), zero);
        assert_eq!(one.i256_div(&zero), zero);
        assert_eq!(zero.i256_div(&one), zero);
    }

    #[test]
    fn mod_i256() {
        let zero = U256::ZERO;
        let one = U256::ONE;
        let one_hundred = U256::from(100);
        let two = U256::from(2);
        let three = U256::from(3);

        let neg_one_hundred = U256::from(100).i256_neg();
        let minus_one = U256::from(1).i256_neg();
        let neg_three = U256::from(3).i256_neg();
        let max_value = U256::from(2).pow(255.into()) - 1;

        // zero
        assert_eq!(minus_one.i256_mod(&U256::ZERO), U256::ZERO);
        assert_eq!(max_value.i256_mod(&U256::ZERO), U256::ZERO);
        assert_eq!(U256::ZERO.i256_mod(&U256::ZERO), U256::ZERO);

        assert_eq!(minus_one.i256_mod(&two), minus_one);
        assert_eq!(U256::I128_MIN.i256_mod(&one), 0);
        assert_eq!(one.i256_mod(&U256::I128_MIN), one);
        assert_eq!(one.i256_mod(&U256::from(i128::MAX)), one);

        assert_eq!(max_value.i256_mod(&minus_one), zero);
        assert_eq!(neg_one_hundred.i256_mod(&minus_one), zero);
        assert_eq!(one_hundred.i256_mod(&two), zero);
        assert_eq!(one_hundred.i256_mod(&neg_three), one);

        assert_eq!(neg_one_hundred.i256_mod(&three), minus_one);

        let a = U256::from(95).i256_neg();
        let b = U256::from(256);
        assert_eq!(a % b, U256::from(161))
    }

    #[test]
    fn negative_i256() {
        assert_eq!(U256::ZERO.i256_neg(), U256::ZERO);

        let one = U256::ONE.i256_neg();
        assert!(one.i256_is_negative());

        let neg_one = U256::from(&[0xff; 32]);
        let pos_one = neg_one.i256_neg();
        assert_eq!(pos_one, U256::ONE);
    }

    #[test]
    fn u256_serde() {
        let encoded = RawBytes::serialize(U256::from(0x4d2)).unwrap();
        let BytesDe(bytes) = encoded.deserialize().unwrap();
        assert_eq!(bytes, &[0x04, 0xd2]);
        let decoded: U256 = encoded.deserialize().unwrap();
        assert_eq!(decoded, 0x4d2);
    }

    #[test]
    fn u256_empty() {
        let encoded = RawBytes::serialize(U256::from(0)).unwrap();
        let BytesDe(bytes) = encoded.deserialize().unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn u256_overflow() {
        let encoded = RawBytes::serialize(BytesSer(&[1; 33])).unwrap();
        encoded
            .deserialize::<U256>()
            .expect_err("should have failed to decode an over-large u256");
    }
}
