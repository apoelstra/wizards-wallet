// Rust Bitcoin Library
// Written in 2014 by
//   Andrew Poelstra <apoelstra@wpsoftware.net>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! # Uint256 type
//!
//! Implementation of a 256-bit ``big integer'' type. The functions here
//! are designed to be fast. There is little attempt to be consistent
//! regarding acting in-place or returning a copy, just whatever is useful.
//!

use std::fmt;
use std::intrinsics;
use std::io::IoResult;
use std::num::Zero;
use std::mem::transmute;

use network::serialize::Serializable;

/// Little-endian 256-bit integer
#[repr(C)]
pub struct Uint256(pub [u64, ..4]);

impl Uint256 {
  /// Constructor
  pub fn from_u64(init: u64) -> Uint256 {
    let val = [init, 0, 0, 0];
    Uint256(val)
  }

  /// Return the least number of bits needed to represent the number
  pub fn bits(&self) -> uint {
    let &Uint256(ref arr) = self;
    if arr[3] > 0 { return 256 - unsafe { intrinsics::ctlz64(arr[3]) } as uint; }
    if arr[2] > 0 { return 192 - unsafe { intrinsics::ctlz64(arr[2]) } as uint; }
    if arr[1] > 0 { return 128 - unsafe { intrinsics::ctlz64(arr[1]) } as uint; }
    return 64 - unsafe { intrinsics::ctlz64(arr[0]) } as uint;
  }

  /// Is bit set?
  pub fn bit_value(&self, index: uint) -> bool {
    let &Uint256(ref arr) = self;
    arr[index / 64] & (1 << (index % 64)) != 0
  }

  /// Shift left
  pub fn shl(&self, shift: uint) -> Uint256 {
    let &Uint256(ref original) = self;
    let mut ret = [0u64, ..4];
    let word_shift = shift / 64;
    let bit_shift = shift % 64;
    for i in range(0u, 4) {
      // Shift
      if bit_shift < 64 && i + word_shift < 4 {
        ret[i + word_shift] += original[i] << bit_shift;
      }
      // Carry
      if bit_shift > 0 && i + word_shift + 1 < 4 {
        ret[i + word_shift + 1] += original[i] >> (64 - bit_shift);
      }
    }
    Uint256(ret)
  }

  /// Shift right
  #[allow(unsigned_negate)]
  pub fn shr(&self, shift: uint) -> Uint256 {
    let &Uint256(ref original) = self;
    let mut ret = [0u64, ..4];
    let word_shift = shift / 64;
    let bit_shift = shift % 64;
    for i in range(0u, 4) {
      // Shift
      if bit_shift < 64 && i - word_shift < 4 {
        ret[i - word_shift] += original[i] >> bit_shift;
      }
      // Carry
      if bit_shift > 0 && i - word_shift - 1 < 4 {
        ret[i - word_shift - 1] += original[i] << (64 - bit_shift);
      }
    }
    Uint256(ret)
  }

  /// Negate
  #[allow(unsigned_negate)]
  pub fn bit_inv(&mut self) {
    let &Uint256(ref mut arr) = self;
    for i in range(0u, 4) {
      arr[i] = !arr[i];
    }
  }

  /// Subtract
  pub fn sub(&self, other: &Uint256) -> Uint256 {
    let mut you = *other;
    you.bit_inv();
    you.increment();
    self.add(&you)
  }

  /// Division
  pub fn div(&self, other: &Uint256) -> Uint256 {
    let mut sub_copy = *self;
    let mut shift_copy = *other;
    let mut ret = [0u64, 0, 0, 0];

    let my_bits = self.bits();
    let your_bits = other.bits();

    // Check for division by 0
    assert!(your_bits != 0);

    // Early return in case we are dividing by a larger number than us
    if my_bits < your_bits {
      return Uint256(ret);
    }

    // Bitwise long division
    let mut shift = my_bits - your_bits;
    shift_copy = shift_copy.shl(shift);
    loop {
      if sub_copy >= shift_copy {
        ret[shift / 64] |= 1 << (shift % 64);
        sub_copy = sub_copy.sub(&shift_copy);
      }
      shift_copy = shift_copy.shr(1);
      if shift == 0 { break; }
      shift -= 1;
    }

    Uint256(ret)
  }

  /// Increment by 1
  pub fn increment(&mut self) {
    let &Uint256(ref mut arr) = self;
    arr[0] += 1;
    if arr[0] == 0 {
      arr[1] += 1;
      if arr[1] == 0 {
        arr[2] += 1;
        if arr[2] == 0 {
          arr[3] += 1;
        }
      }
    }
  }

  /// Multiplication by u32
  pub fn mul_u32(&self, other: u32) -> Uint256 {
    let &Uint256(ref arr) = self;
    let mut carry = [0u64, 0, 0, 0];
    let mut ret = [0u64, 0, 0, 0];
    for i in range(0u, 4) {
      let upper = other as u64 * (arr[i] >> 32);
      let lower = other as u64 * (arr[i] & 0xFFFFFFFF);
      if i < 3 {
        carry[i + 1] += upper >> 32;
      }
      ret[i] = lower + (upper << 32);
    }
    Uint256(ret).add(&Uint256(carry))
  }

  /// Bitwise and with `n` ones
  pub fn mask(&self, n: uint) -> Uint256 {
    let &Uint256(ref arr) = self;
    match n {
      n if n < 0x40 => Uint256([arr[0] & ((1 << n) - 1), 0, 0, 0]),
      n if n < 0x80 => Uint256([arr[0], arr[1] & ((1 << (n - 0x40)) - 1), 0, 0]),
      n if n < 0xC0 => Uint256([arr[0], arr[1], arr[2] & ((1 << (n - 0x80)) - 1), 0]),
      n if n < 0x100 => Uint256([arr[0], arr[1], arr[2], arr[3] & ((1 << (n - 0xC0)) - 1)]),
      _ => *self
    }
  }

  /// Returns a number which is just the bits from start to end
  pub fn bit_slice(&self, start: uint, end: uint) -> Uint256 {
    self.shr(start).mask(end - start)
  }

  /// Bitwise and
  pub fn and(&self, other: &Uint256) -> Uint256 {
    let &Uint256(ref arr1) = self;
    let &Uint256(ref arr2) = other;
    Uint256([arr1[0] & arr2[0],
             arr1[1] & arr2[1],
             arr1[2] & arr2[2],
             arr1[3] & arr2[3]])
  }

  /// Bitwise xor
  pub fn xor(&self, other: &Uint256) -> Uint256 {
    let &Uint256(ref arr1) = self;
    let &Uint256(ref arr2) = other;
    Uint256([arr1[0] ^ arr2[0],
             arr1[1] ^ arr2[1],
             arr1[2] ^ arr2[2],
             arr1[3] ^ arr2[3]])
  }

  /// Trailing zeros
  pub fn trailing_zeros(&self) -> uint {
    let &Uint256(ref arr) = self;
    if arr[0] > 0 { return arr[0].trailing_zeros() as uint; }
    if arr[1] > 0 { return 0x40 + arr[1].trailing_zeros() as uint; }
    if arr[2] > 0 { return 0x80 + arr[2].trailing_zeros() as uint; }
    0xC0 + arr[3].trailing_zeros() as uint
  }
}

impl Add<Uint256,Uint256> for Uint256 {
  fn add(&self, other: &Uint256) -> Uint256 {
    let &Uint256(ref me) = self;
    let &Uint256(ref you) = other;
    let mut ret = [0u64, 0, 0, 0];
    let mut carry = [0u64, 0, 0, 0];
    let mut b_carry = false;
    for i in range(0u, 4) {
      ret[i] = me[i] + you[i];
      if i < 3 && ret[i] < me[i] {
        carry[i + 1] = 1;
        b_carry = true;
      }
    }
    if b_carry { Uint256(ret).add(&Uint256(carry)) } else { Uint256(ret) }
  }
}

impl Zero for Uint256 {
  fn zero() -> Uint256 { Uint256::from_u64(0) }
  fn is_zero(&self) -> bool {
    let &Uint256(ref arr) = self;
    arr[0] == 0 && arr[1] == 0 && arr[2] == 0 && arr[3] == 0
  }
}

impl PartialEq for Uint256 {
  fn eq(&self, other: &Uint256) -> bool {
    let &Uint256(ref arr1) = self;
    let &Uint256(ref arr2) = other;
    (arr1[0] == arr2[0]) && (arr1[1] == arr2[1]) &&
      (arr1[2] == arr2[2]) && (arr1[3] == arr2[3])
  }
}

impl Eq for Uint256 {}

impl PartialOrd for Uint256 {
  fn partial_cmp(&self, other: &Uint256) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Uint256 {
  fn cmp(&self, other: &Uint256) -> Ordering {
    let &Uint256(ref me) = self;
    let &Uint256(ref you) = other;
    for i in range(0, 4) {
      if me[3 - i] < you[3 - i] { return Less; }
      if me[3 - i] > you[3 - i] { return Greater; }
    }
    return Equal;
  }
}

impl fmt::Show for Uint256 {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "{}", self.serialize().as_slice())
  }
}

impl Serializable for Uint256 {
  fn serialize(&self) -> Vec<u8> {
    let vec = unsafe { transmute::<Uint256, [u8, ..32]>(*self) };
    vec.serialize()
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<Uint256> {
    let ret: [u8, ..32] = try!(Serializable::deserialize(iter.by_ref()));
    Ok(unsafe { transmute::<[u8, ..32], Uint256>(ret) })
  }
}

#[cfg(test)]
mod tests {
  use std::prelude::*;
  use std::io::IoResult;

  use network::serialize::Serializable;
  use util::uint256::Uint256;

  #[test]
  pub fn uint256_bits_test() {
    assert_eq!(Uint256::from_u64(255).bits(), 8);
    assert_eq!(Uint256::from_u64(256).bits(), 9);
    assert_eq!(Uint256::from_u64(300).bits(), 9);
    assert_eq!(Uint256::from_u64(60000).bits(), 16);
    assert_eq!(Uint256::from_u64(70000).bits(), 17);

    // Try to read the following lines out loud quickly
    let mut shl = Uint256::from_u64(70000);
    shl = shl.shl(100);
    assert_eq!(shl.bits(), 117);
    shl = shl.shl(100);
    assert_eq!(shl.bits(), 217);
    shl = shl.shl(100);
    assert_eq!(shl.bits(), 0);

    // Bit set check
    assert!(!Uint256::from_u64(10).bit_value(0));
    assert!(Uint256::from_u64(10).bit_value(1));
    assert!(!Uint256::from_u64(10).bit_value(2));
    assert!(Uint256::from_u64(10).bit_value(3));
    assert!(!Uint256::from_u64(10).bit_value(4));
  }

  #[test]
  pub fn uint256_comp_test() {
    let small = Uint256([10u64, 0, 0, 0]);
    let big = Uint256([0x8C8C3EE70C644118u64, 0x0209E7378231E632, 0, 0]);
    let bigger = Uint256([0x9C8C3EE70C644118u64, 0x0209E7378231E632, 0, 0]);
    let biggest = Uint256([0x5C8C3EE70C644118u64, 0x0209E7378231E632, 0, 1]);

    assert!(small < big);
    assert!(big < bigger);
    assert!(bigger < biggest);
    assert!(bigger <= biggest);
    assert!(biggest <= biggest);
    assert!(bigger >= big);
    assert!(bigger >= small);
    assert!(small <= small);
  }

  #[test]
  pub fn uint256_arithmetic_test() {
    let init = Uint256::from_u64(0xDEADBEEFDEADBEEF);
    let copy = init;

    let add = init.add(&copy);
    assert_eq!(add, Uint256([0xBD5B7DDFBD5B7DDEu64, 1, 0, 0]));
    // Bitshifts
    let shl = add.shl(88);
    assert_eq!(shl, Uint256([0u64, 0xDFBD5B7DDE000000, 0x1BD5B7D, 0]));
    let shr = shl.shr(40);
    assert_eq!(shr, Uint256([0x7DDE000000000000u64, 0x0001BD5B7DDFBD5B, 0, 0]));
    // Increment
    let mut incr = shr;
    incr.increment();
    assert_eq!(incr, Uint256([0x7DDE000000000001u64, 0x0001BD5B7DDFBD5B, 0, 0]));
    // Subtraction
    let sub = incr.sub(&init);
    assert_eq!(sub, Uint256([0x9F30411021524112u64, 0x0001BD5B7DDFBD5A, 0, 0]));
    // Multiplication
    let mult = sub.mul_u32(300);
    assert_eq!(mult, Uint256([0x8C8C3EE70C644118u64, 0x0209E7378231E632, 0, 0]));
    // Division
    assert_eq!(Uint256::from_u64(105).div(&Uint256::from_u64(5)), Uint256::from_u64(21));
    let div = mult.div(&Uint256::from_u64(300));
    assert_eq!(div, Uint256([0x9F30411021524112u64, 0x0001BD5B7DDFBD5A, 0, 0]));
    // TODO: bit inversion
  }

  #[test]
  pub fn uint256_extreme_bitshift_test() {
    // Shifting a u64 by 64 bits gives an undefined value, so make sure that
    // we're doing the Right Thing here
    let init = Uint256::from_u64(0xDEADBEEFDEADBEEF);

    assert_eq!(init.shl(64), Uint256([0, 0xDEADBEEFDEADBEEF, 0, 0]));
    let add = init.shl(64).add(&init);
    assert_eq!(add, Uint256([0xDEADBEEFDEADBEEF, 0xDEADBEEFDEADBEEF, 0, 0]));
    assert_eq!(add.shr(0), Uint256([0xDEADBEEFDEADBEEF, 0xDEADBEEFDEADBEEF, 0, 0]));
    assert_eq!(add.shl(0), Uint256([0xDEADBEEFDEADBEEF, 0xDEADBEEFDEADBEEF, 0, 0]));
    assert_eq!(add.shr(64), Uint256([0xDEADBEEFDEADBEEF, 0, 0, 0]));
    assert_eq!(add.shl(64), Uint256([0, 0xDEADBEEFDEADBEEF, 0xDEADBEEFDEADBEEF, 0]));
  }

  #[test]
  pub fn uint256_serialize_test() {
    let start1 = Uint256([0x8C8C3EE70C644118u64, 0x0209E7378231E632, 0, 0]);
    let start2 = Uint256([0x8C8C3EE70C644118u64, 0x0209E7378231E632, 0xABCD, 0xFFFF]);
    let serial1 = start1.serialize();
    let serial2 = start2.serialize();
    let end1: IoResult<Uint256> = Serializable::deserialize(serial1.iter().map(|n| *n));
    let end2: IoResult<Uint256> = Serializable::deserialize(serial2.iter().map(|n| *n));

    assert_eq!(end1, Ok(start1));
    assert_eq!(end2, Ok(start2));
  }
}

