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

use blockdata::transaction::{Transaction, TxOut};
use blockdata::block::Block;
use network::serialize::{Serializable, SerializeIter};
use util::hash::Sha256dHash;
use util::patricia_tree::PatriciaTree;

/// How much of the hash to use as a key
static KEY_LEN: uint = 120;

/// Vector of outputs; None indicates a nonexistent or already spent output
type UtxoNode = Vec<Option<TxOut>>;

/// The UTXO set
pub struct UtxoSet {
  tree: PatriciaTree<UtxoNode>,
  last_hash: Sha256dHash,
  n_utxos: u64
}

impl_serializable!(UtxoSet, last_hash, n_utxos, tree)

impl UtxoSet {
  /// Constructs a new UTXO set
  pub fn new(genesis: Block) -> UtxoSet {
    // There is in fact a transaction in the genesis block, but the Bitcoin
    // reference client does not add its sole output to the UTXO set. We
    // must follow suit, otherwise we will accept a transaction spending it
    // while the reference client won't, causing us to fork off the network.
    UtxoSet {
      tree: PatriciaTree::new(),
      last_hash: genesis.header.hash(),
      n_utxos: 0
    }
  }

  /// Add a new UTXO to the set
  fn add_utxo(&mut self, txo: TxOut, txid: Sha256dHash, vout: u32) -> bool {
    let txid = txid.as_uint256();
    // Locate node if it's already there
    {
      match self.tree.lookup_mut(&txid, KEY_LEN) {
        Some(node) => {
          // Insert the output
          node.grow_set(vout as uint, &None, Some(txo));
          // Return success
          return true;
        }
        None => {}
      };
    }
    // If we haven't returned yet, the node wasn't there. So insert it.
    let mut new_node = vec![];
    new_node.grow_set(vout as uint, &None, Some(txo));
    self.tree.insert(&txid, KEY_LEN, new_node);
    // Return success
    return true;
  }

  /// Add all the UTXOs of a transaction to the set
  fn add_utxos(&mut self, tx: &Transaction) -> bool {
    let txid = tx.hash();
    // Locate node if it's already there
    {
      match self.tree.lookup_mut(&txid.as_uint256(), KEY_LEN) {
        Some(node) => {
          node.reserve(tx.output.len());
          // Insert the output
          for (vout, txo) in tx.output.iter().enumerate() {
            node.grow_set(vout as uint, &None, Some(txo.clone()));
          }
          // Return success
          return true;
        }
        None => {}
      };
    }
    // If we haven't returned yet, the node wasn't there. So insert it.
    let mut new_node = Vec::with_capacity(tx.output.len());
    self.n_utxos += tx.output.len() as u64;
    for (vout, txo) in tx.output.iter().enumerate() {
      new_node.grow_set(vout as uint, &None, Some(txo.clone()));
    }
    self.tree.insert(&txid.as_uint256(), KEY_LEN, new_node);
    // Return success
    return true;
  }

  /// Remove a UTXO from the set and return it
  fn take_utxo(&mut self, txid: Sha256dHash, vout: u32) -> Option<TxOut> {
    // This whole function has awkward scoping thx to lexical borrow scoping :(
    let (ret, should_delete) = {
      // Locate the UTXO, failing if not found
      let node = match self.tree.lookup_mut(&txid.as_uint256(), KEY_LEN) {
        Some(node) => node,
        None => return None
      };

      let ret = {
        // Check that this specific output is there
        if vout as uint >= node.len() { return None; }
        let replace = node.get_mut(vout as uint);
        replace.take()
      };

      let should_delete = node.iter().filter(|slot| slot.is_some()).count() == 0;
      (ret, should_delete)
    };

    // Delete the whole node if it is no longer being used
    if should_delete {
      self.tree.delete(&txid.as_uint256(), KEY_LEN);
    }

    self.n_utxos -= if ret.is_some() { 1 } else { 0 };
    ret
  }

  /// Determine whether a UTXO is in the set
  fn get_utxo<'a>(&'a mut self, txid: Sha256dHash, vout: u32) -> Option<&'a TxOut> {
    // Locate the UTXO, failing if not found
    let node = match self.tree.lookup_mut(&txid.as_uint256(), KEY_LEN) {
      Some(node) => node,
      None => return None
    };
    // Check that this specific output is there
    if vout as uint >= node.len() { return None; }
    let replace = node.get_mut(vout as uint);
    replace.as_ref()
  }

  /// Apply the transactions contained in a block
  pub fn update(&mut self, block: &Block) -> bool {
    fn unwind(set: &mut UtxoSet, block: &Block, n_txes: uint) {
      for tx in block.txdata.iter().take(n_txes) {
        // Unwind all added outputs
        let tx_hash = tx.hash();
        for (n, _) in tx.output.iter().enumerate() {
          set.take_utxo(tx_hash, n as u32);
        }
      }
    }

    for (n_tx, tx) in block.txdata.iter().enumerate() {
      // Check if we can remove inputs (except for the coinbase)
      // We need to do this check before actually removing them since we
      // can't put them back if we have to unwind (we could put them on
      // a stack, I guess, but that's slow).
      if n_tx > 0 {
        for input in tx.input.iter() {
          if self.get_utxo(input.prev_hash, input.prev_index).is_none() {
            unwind(self, block, n_tx);
            return false; 
          }
        }
      }

      // Add outputs
      self.add_utxos(tx);
    }
    // Actually remove the inputs
    for tx in block.txdata.iter().skip(1) {
      for input in tx.input.iter() {
        self.take_utxo(input.prev_hash, input.prev_index);
      }
    }
    self.last_hash = block.header.hash();
    true
  }

  /// Get the hash of the last block added to the utxo set
  pub fn last_hash(&self) -> Sha256dHash {
    self.last_hash
  }

  /// Get the number of UTXOs in the set
  pub fn n_utxos(&self) -> uint {
    self.n_utxos as uint
  }

  /// Get the number of UTXOs in the set
  pub fn tree_size(&self) -> uint {
    self.tree.node_count()
  }
}

#[cfg(test)]
mod tests {
  use std::prelude::*;
  use std::io::IoResult;

  use util::hash::Sha256dHash;
  use blockdata::constants::genesis_block;
  use blockdata::script::Script;
  use blockdata::transaction::TxOut;
  use blockdata::utxoset::UtxoSet;
  use network::serialize::Serializable;

  #[test]
  fn utxoset_serialize_test() {
    // el-cheapo rng
    fn rand(n: uint) -> uint { n * 53 % 23 }

    let mut empty_set = UtxoSet::new(genesis_block());
    let mut hashes = vec![];

    for i in range(0u, 5) {
      let hash = Sha256dHash::from_data(&[(i / 0x100) as u8, (i % 0x100) as u8]);
      empty_set.add_utxo(TxOut { value: rand(rand(i)) as u64, script_pubkey: Script::new() }, hash, rand(i) as u32);
      hashes.push((rand(i) as u32, hash));
    }

    let serial = empty_set.serialize();
    assert_eq!(serial, empty_set.serialize_iter().collect());

    let deserial: IoResult<UtxoSet> = Serializable::deserialize(serial.iter().map(|n| *n));
    assert!(deserial.is_ok());

    let mut read_set = deserial.unwrap();
    for &(n, hash) in hashes.iter() {
      let expected = Some(TxOut { value: rand(n as uint) as u64, script_pubkey: Script::new() });
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



