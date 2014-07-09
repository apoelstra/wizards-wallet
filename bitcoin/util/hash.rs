// Rust Bitcoin Library
// Written in 2014 by
//   Andrew Poelstra <apoelstra@wpsoftware.net>
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! # Hash functions
//!
//! Utility functions related to hashing data, including merkleization

use collections::bitv::{Bitv, from_bytes};
use core::char::from_digit;
use core::cmp::min;
use std::fmt;
use std::io::{IoResult, IoError, InvalidInput};
use std::mem::transmute;

use crypto::digest::Digest;
use crypto::sha2;

use network::serialize::Serializable;
use util::iter::FixedTakeable;
use util::uint256::Uint256;

/// A Bitcoin hash, 32-bytes, computed from x as SHA256(SHA256(x))
pub struct Sha256dHash([u8, ..32]);

/// Returns the all-zeroes "hash"
pub fn zero_hash() -> Sha256dHash { Sha256dHash([0u8, ..32]) }

impl Sha256dHash {
  /// Create a hash by hashing some data
  pub fn from_data(data: &[u8]) -> Sha256dHash {
    let Sha256dHash(mut ret) = zero_hash();
    let mut sha2 = sha2::Sha256::new();
    sha2.input(data);
    sha2.result(ret.as_mut_slice());
    sha2.reset();
    sha2.input(ret.as_slice());
    sha2.result(ret.as_mut_slice());
    Sha256dHash(ret)
  }

  /// Returns a slice containing the bytes of the has
  pub fn as_slice<'a>(&'a self) -> &'a [u8] {
    let &Sha256dHash(ref data) = self;
    data.as_slice()
  }

  /// Converts a hash to a bit vector
  pub fn as_bitv(&self) -> Bitv {
    from_bytes(self.as_slice())
  }

  /// Converts a hash to a Uint256, interpreting it as a little endian encoding.
  pub fn as_uint256(&self) -> Uint256 {
    let &Sha256dHash(data) = self;
    unsafe { Uint256(transmute(data)) }
  }
}

impl Clone for Sha256dHash {
  fn clone(&self) -> Sha256dHash {
    *self
  }
}

impl PartialEq for Sha256dHash {
  fn eq(&self, other: &Sha256dHash) -> bool {
    let &Sha256dHash(ref mydata) = self;
    let &Sha256dHash(ref yourdata) = other;
    for i in range(0u, 32) {
      if mydata[i] != yourdata[i] {
        return false;
      }
    }
    return true;
  }
}

impl Serializable for Sha256dHash {
  fn serialize(&self) -> Vec<u8> {
    let &Sha256dHash(ref data) = self;
    data.iter().map(|n| *n).collect()
  }

  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<Sha256dHash> {
    let Sha256dHash(mut ret) = zero_hash();
    let mut fixediter = iter.enumerate().fixed_take(32);
    for (n, data) in fixediter {
      ret[n] = data;
    }
    match fixediter.is_err() {
      false => Ok(Sha256dHash(ret)),
      true => Err(IoError {
        kind: InvalidInput,
        desc: "unexpected end of input",
        detail: Some(format!("Need 32 bytes, was {:} short.", fixediter.remaining()))
      })
    }
  }
}

impl fmt::LowerHex for Sha256dHash {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    let &Sha256dHash(ref data) = self;
    let mut rv = [0, ..64];
    let mut hex = data.iter().rev().map(|n| *n).enumerate();
    for (i, ch) in hex {
      rv[2*i]     = from_digit(ch as uint / 16, 16).unwrap() as u8;
      rv[2*i + 1] = from_digit(ch as uint % 16, 16).unwrap() as u8;
    }
    f.write(rv.as_slice())
  }
}

impl fmt::Show for Sha256dHash {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "{:x}", *self)
  }
}

//TODO: this should be an impl and the function have first parameter self.
//See https://github.com/rust-lang/rust/issues/15060 for why this isn't so.
//impl<T: Serializable> Vec<T> {
  /// Construct a merkle tree from a vector, with elements ordered as
  /// they were in the original vector, and return the merkle root.
  pub fn merkle_root<T: Serializable>(data: &[T]) -> Sha256dHash {
    fn merkle_root(data: Vec<Sha256dHash>) -> Sha256dHash {
      // Base case
      if data.len() < 1 {
        return zero_hash();
      }
      if data.len() < 2 {
        return *data.get(0);
      }
      // Recursion
      let mut next = vec![];
      for idx in range(0, (data.len() + 1) / 2) {
        let idx1 = 2 * idx;
        let idx2 = min(idx1 + 1, data.len() - 1);
        let to_hash = data.get(idx1).hash().serialize().append(data.get(idx2).hash().serialize().as_slice());
        next.push(to_hash.hash());
      }
      merkle_root(next)
    }
    merkle_root(data.iter().map(|obj| obj.hash()).collect())
  }
//}


#[cfg(test)]
mod tests {
  use std::prelude::*;
  use collections::bitv::from_bytes;

  use util::hash::Sha256dHash;
  use util::misc::hex_bytes;

  #[test]
  fn test_sha256d() {
    assert_eq!(Sha256dHash::from_data(&[]).as_slice(),
               hex_bytes("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap().as_slice());
    assert_eq!(Sha256dHash::from_data(b"TEST").as_slice(),
               hex_bytes("d7bd34bfe44a18d2aa755a344fe3e6b06ed0473772e6dfce16ac71ba0b0a241c").unwrap().as_slice());
  }

  #[test]
  fn test_hash_to_bitvset() {
    assert_eq!(Sha256dHash::from_data(&[]).as_bitv(),
               from_bytes(hex_bytes("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap().as_slice()));
  }
}

