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

//! # UTXO Set
//!
//! This module provides the structures and functions to maintain an
//! index of UTXOs.
//!

use std::io::IoResult;

use blockdata::transaction::TxOut;
use network::serialize::{Serializable, SerializeIter};
use util::hash::Sha256dHash;
use util::patricia_tree::PatriciaTree;

/// Vector of outputs; None indicates a nonexistent or already spent output
type UtxoNode = Vec<Option<TxOut>>;

/// The UTXO set
pub struct UtxoSet {
  tree: PatriciaTree<UtxoNode>
}

impl_serializable!(UtxoSet, tree)

impl UtxoSet {
  /// Constructs a new UTXO set
  pub fn new() -> UtxoSet { UtxoSet { tree: PatriciaTree::new() } }

  /// Add a new UTXO to the set
  pub fn add_utxo(&mut self, txo: TxOut, txid: Sha256dHash, vout: uint) -> bool {
    let txid = txid.as_bitv();
    // Locate node if it's already there
    {
      match self.tree.lookup_mut(&txid) {
        Some(node) => {
          // Insert the output
          node.grow_set(vout, &None, Some(txo));
          // Return success
          return true;
        }
        None => {}
      };
    }
    // If we haven't returned yet, the node wasn't there. So insert it.
    let mut new_node = vec![];
    new_node.grow_set(vout, &None, Some(txo));
    self.tree.insert(&txid, new_node);
    // Return success
    return true;
  }

  /// Remove a UTXO from the set and return it
  pub fn take_utxo(&mut self, txid: Sha256dHash, vout: uint) -> Option<TxOut> {
    // Locate the UTXO, failing if not found
    let node = match self.tree.lookup_mut(&txid.as_bitv()) {
      Some(node) => node,
      None => return None
    };
    // Check that this specific output is there
    if vout >= node.len() { return None; }
    let replace = node.get_mut(vout);
    replace.take()
  }
}

#[cfg(test)]
mod tests {
  use std::prelude::*;
  use std::io::IoResult;

  use util::hash::Sha256dHash;
  use blockdata::script::Script;
  use blockdata::transaction::TxOut;
  use blockdata::utxoset::UtxoSet;
  use network::serialize::Serializable;

  #[test]
  fn utxoset_serialize_test() {
    // el-cheapo rng
    fn rand(n: uint) -> uint { n * 53 % 23 }

    let mut empty_set = UtxoSet::new();
    let mut hashes = vec![];

    for i in range(0u, 5000) {
      let hash = Sha256dHash::from_data(&[(i / 0x100) as u8, (i % 0x100) as u8]);
      empty_set.add_utxo(TxOut { value: rand(rand(i)) as u64, script_pubkey: Script::new() }, hash, rand(i));
      hashes.push((rand(i), hash));
    }


    let serial = empty_set.serialize();
    assert_eq!(serial, empty_set.serialize_iter().collect());

    let deserial: IoResult<UtxoSet> = Serializable::deserialize(serial.iter().map(|n| *n));
    assert!(deserial.is_ok());

    let mut read_set = deserial.unwrap();
    for &(n, hash) in hashes.iter() {
      let expected = Some(TxOut { value: rand(n) as u64, script_pubkey: Script::new() });
      // Try taking non-existent UTXO
      assert_eq!(read_set.take_utxo(hash, 100 + n), None);
      // Check take of real UTXO
      let ret = read_set.take_utxo(hash, n);
      assert_eq!(ret, expected);
      // Try double-take
      assert_eq!(read_set.take_utxo(hash, n), None);
    }
  }
}



