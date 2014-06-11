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

use crypto::digest::Digest;
use crypto::sha2;

use network::serialize::Serializable;
use util::iter::FixedTakeable;
#[cfg(test)]
use util::misc::hex_bytes;

use std::io::{IoResult, InvalidInput, standard_error};

pub struct Sha256dHash {
  data: [u8, ..32]
}

pub fn zero_hash() -> Sha256dHash { Sha256dHash { data: [0u8, ..32] } }

impl Sha256dHash {
  pub fn from_data(data: &[u8]) -> Sha256dHash {
    let mut ret = zero_hash();
    let mut sha2 = sha2::Sha256::new();
    sha2.input(data);
    sha2.result(ret.data.as_mut_slice());
    sha2.reset();
    sha2.input(ret.data.as_slice());
    sha2.result(ret.data.as_mut_slice());
    ret
  }

  pub fn data<'a>(&'a self) -> &'a [u8] {
    self.data.as_slice()
  }
}

impl Clone for Sha256dHash {
  fn clone(&self) -> Sha256dHash {
    *self
  }
}

impl Serializable for Sha256dHash {
  fn serialize(&self) -> Vec<u8> {
    self.data.iter().rev().map(|n| *n).collect()
  }

  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<Sha256dHash> {
    let mut ret = zero_hash();
    let mut fixediter = iter.enumerate().fixed_take(32);
    for (n, data) in fixediter {
      ret.data[32 - n - 1] = data;
    }
    match fixediter.is_err() {
      false => Ok(ret),
      true => Err(standard_error(InvalidInput))
    }
  }
}

#[test]
fn test_sha256d() {
  assert_eq!(Sha256dHash::from_data(&[]).data().as_slice(),
             hex_bytes("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap().as_slice());
  assert_eq!(Sha256dHash::from_data(bytes!("TEST")).data().as_slice(),
             hex_bytes("d7bd34bfe44a18d2aa755a344fe3e6b06ed0473772e6dfce16ac71ba0b0a241c").unwrap().as_slice());
}



