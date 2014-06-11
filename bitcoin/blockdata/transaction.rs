/// Rust Bitcoin Library
/// Written in 2014 by
///   Andrew Poelstra <apoelstra@wpsoftware.net>
///
/// To the extent possible under law, the author(s) have dedicated all
/// copyright and related and neighboring rights to this software to
/// the public domain worldwide. This software is distributed without
/// any warranty.
///
/// You should have received a copy of the CC0 Public Domain Dedication
/// along with this software.
/// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
///

use std::io::IoResult;
use util::hash::Sha256dHash;
use network::serialize::Serializable;
use blockdata::script::Script;
#[cfg(test)]
use util::misc::hex_bytes;

pub struct TxIn {
  pub prev_hash: Sha256dHash,
  pub prev_index: u32,
  pub script_sig: Script,
  pub sequence: u32,
}

pub struct TxOut {
  pub value: u64,
  pub script_pubkey: Script
}

pub struct Transaction {
  pub version: u32,
  pub lock_time: u32,
  pub input: Vec<TxIn>,
  pub output: Vec<TxOut>
}

impl Serializable for TxIn {
  fn serialize(&self) -> Vec<u8> {
    let mut ret = self.prev_hash.serialize();
    ret.extend(self.prev_index.serialize().move_iter());
    ret.extend(self.script_sig.serialize().move_iter());
    ret.extend(self.sequence.serialize().move_iter());
    ret
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<TxIn> {
    Ok(TxIn {
      prev_hash: try!(Serializable::deserialize(iter.by_ref())),
      prev_index: try!(Serializable::deserialize(iter.by_ref())),
      script_sig: try!(Serializable::deserialize(iter.by_ref())),
      sequence: try!(Serializable::deserialize(iter.by_ref()))
    })
  }
}

impl Serializable for TxOut {
  fn serialize(&self) -> Vec<u8> {
    let mut ret = self.value.serialize();
    ret.extend(self.script_pubkey.serialize().move_iter());
    ret
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<TxOut> {
    Ok(TxOut {
      value: try!(Serializable::deserialize(iter.by_ref())),
      script_pubkey: try!(Serializable::deserialize(iter.by_ref()))
    })
  }
}

impl Serializable for Transaction {
  fn serialize(&self) -> Vec<u8> {
    let mut ret = self.version.serialize();
    ret.extend(self.input.serialize().move_iter());
    ret.extend(self.output.serialize().move_iter());
    ret.extend(self.lock_time.serialize().move_iter());
    ret
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<Transaction> {
    Ok(Transaction {
      version: try!(Serializable::deserialize(iter.by_ref())),
      input: try!(Serializable::deserialize(iter.by_ref())),
      output: try!(Serializable::deserialize(iter.by_ref())),
      lock_time: try!(Serializable::deserialize(iter.by_ref()))
    })
  }
}

impl Transaction {
  pub fn hash(&self) -> Sha256dHash { Sha256dHash::from_data(self.serialize().as_slice()) }
}


#[test]
fn test_txin() {
  let txin: IoResult<TxIn> = Serializable::deserialize(hex_bytes("a15d57094aa7a21a28cb20b59aab8fc7d1149a3bdbcddba9c622e4f5f6a99ece010000006c493046022100f93bb0e7d8db7bd46e40132d1f8242026e045f03a0efe71bbb8e3f475e970d790221009337cd7f1f929f00cc6ff01f03729b069a7c21b59b1736ddfee5db5946c5da8c0121033b9b137ee87d5a812d6f506efdd37f0affa7ffc310711c06c7f3e097c9447c52ffffffff").unwrap().iter().map(|n| *n));
  assert!(txin.is_ok());
}

#[test]
fn test_transaction() {
  let hex_tx = hex_bytes("0100000001a15d57094aa7a21a28cb20b59aab8fc7d1149a3bdbcddba9c622e4f5f6a99ece010000006c493046022100f93bb0e7d8db7bd46e40132d1f8242026e045f03a0efe71bbb8e3f475e970d790221009337cd7f1f929f00cc6ff01f03729b069a7c21b59b1736ddfee5db5946c5da8c0121033b9b137ee87d5a812d6f506efdd37f0affa7ffc310711c06c7f3e097c9447c52ffffffff0100e1f505000000001976a9140389035a9225b3839e2bbf32d826a1e222031fd888ac00000000").unwrap();
  let tx: IoResult<Transaction> = Serializable::deserialize(hex_tx.iter().map(|n| *n));
  assert!(tx.is_ok());
  let realtx = tx.unwrap();
  // All these tests aren't really needed because if they fail, the hash check at the end
  // will also fail. But these will show you where the failure is so I'll leave them in.
  assert_eq!(realtx.version, 1);
  assert_eq!(realtx.input.len(), 1);
  // In particular this one is easy to get backward -- in bitcoin hashes are encoded
  // as little-endian 256-bit numbers rather than as data strings.
  assert_eq!(realtx.input.get(0).prev_hash.data().as_slice(), hex_bytes("ce9ea9f6f5e422c6a9dbcddb3b9a14d1c78fab9ab520cb281aa2a74a09575da1").unwrap().as_slice());
  assert_eq!(realtx.input.get(0).prev_index, 1);
  assert_eq!(realtx.output.len(), 1);
  assert_eq!(realtx.lock_time, 0);

  assert_eq!(realtx.hash().serialize().as_slice(), hex_bytes("a6eab3c14ab5272a58a5ba91505ba1a4b6d7a3a9fcbd187b6cd99a7b6d548cb7").unwrap().as_slice());
}



