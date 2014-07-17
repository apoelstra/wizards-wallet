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
use util::uint::Uint128;
use util::patricia_tree::PatriciaTree;
use util::thinvec::ThinVec;

/// How much of the hash to use as a key
static KEY_LEN: uint = 128;

/// Vector of outputs; None indicates a nonexistent or already spent output
type UtxoNode = ThinVec<Option<Box<TxOut>>>;

/// The UTXO set
pub struct UtxoSet {
  // We use a 128-bit indexed tree to save memory
  tree: PatriciaTree<UtxoNode, Uint128>,
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

  /// Add all the UTXOs of a transaction to the set
  fn add_utxos(&mut self, tx: &Transaction) -> bool {
    let txid = tx.hash();
    // Locate node if it's already there
    {
      match self.tree.lookup_mut(&txid.as_uint128(), KEY_LEN) {
        Some(node) => {
          node.reserve(tx.output.len() as u32);
          // Insert the output
          for (vout, txo) in tx.output.iter().enumerate() {
            // Unsafe since if node has not yet been initialized, overwriting
            // a mutable pointer like this would cause uninitialized data to
            // be dropped.
            unsafe { *node.get_mut(vout as uint) = Some(box txo.clone()); }
          }
          // Return success
          return true;
        }
        None => {}
      };
    }
    // If we haven't returned yet, the node wasn't there. So insert it.
    let mut new_node = ThinVec::with_capacity(tx.output.len() as u32);
    self.n_utxos += tx.output.len() as u64;
    for (vout, txo) in tx.output.iter().enumerate() {
      // Unsafe since we are not uninitializing the old data in the vector
      unsafe { new_node.init(vout as uint, Some(box txo.clone())); }
    }
    self.tree.insert(&txid.as_uint128(), KEY_LEN, new_node);
    // Return success
    return true;
  }

  /// Remove a UTXO from the set and return it
  fn take_utxo(&mut self, txid: Sha256dHash, vout: u32) -> Option<Box<TxOut>> {
    // This whole function has awkward scoping thx to lexical borrow scoping :(
    let (ret, should_delete) = {
      // Locate the UTXO, failing if not found
      let node = match self.tree.lookup_mut(&txid.as_uint128(), KEY_LEN) {
        Some(node) => node,
        None => return None
      };

      let ret = {
        // Check that this specific output is there
        if vout as uint >= node.len() { return None; }
        let replace = unsafe { node.get_mut(vout as uint) };
        replace.take()
      };

      let should_delete = node.iter().filter(|slot| slot.is_some()).count() == 0;
      (ret, should_delete)
    };

    // Delete the whole node if it is no longer being used
    if should_delete {
      self.tree.delete(&txid.as_uint128(), KEY_LEN);
    }

    self.n_utxos -= if ret.is_some() { 1 } else { 0 };
    ret
  }

  /// Determine whether a UTXO is in the set
  fn get_utxo<'a>(&'a mut self, txid: Sha256dHash, vout: u32) -> Option<&'a Box<TxOut>> {
    // Locate the UTXO, failing if not found
    let node = match self.tree.lookup_mut(&txid.as_uint128(), KEY_LEN) {
      Some(node) => node,
      None => return None
    };
    // Check that this specific output is there
    if vout as uint >= node.len() { return None; }
    let replace = unsafe { node.get(vout as uint) };
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
  use serialize::hex::FromHex;

  use blockdata::constants::genesis_block;
  use blockdata::block::Block;
  use blockdata::utxoset::UtxoSet;
  use network::serialize::Serializable;

  #[test]
  fn utxoset_serialize_test() {
    let mut empty_set = UtxoSet::new(genesis_block());

    let new_block: Block = Serializable::deserialize("010000004ddccd549d28f385ab457e98d1b11ce80bfea2c5ab93015ade4973e400000000bf4473e53794beae34e64fccc471dace6ae544180816f89591894e0f417a914cd74d6e49ffff001d323b3a7b0201000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0804ffff001d026e04ffffffff0100f2052a0100000043410446ef0102d1ec5240f0d061a4246c1bdef63fc3dbab7733052fbbf0ecd8f41fc26bf049ebb4f9527f374280259e7cfa99c48b0e3f39c51347a19a5819651503a5ac00000000010000000321f75f3139a013f50f315b23b0c9a2b6eac31e2bec98e5891c924664889942260000000049483045022100cb2c6b346a978ab8c61b18b5e9397755cbd17d6eb2fe0083ef32e067fa6c785a02206ce44e613f31d9a6b0517e46f3db1576e9812cc98d159bfdaf759a5014081b5c01ffffffff79cda0945903627c3da1f85fc95d0b8ee3e76ae0cfdc9a65d09744b1f8fc85430000000049483045022047957cdd957cfd0becd642f6b84d82f49b6cb4c51a91f49246908af7c3cfdf4a022100e96b46621f1bffcf5ea5982f88cef651e9354f5791602369bf5a82a6cd61a62501fffffffffe09f5fe3ffbf5ee97a54eb5e5069e9da6b4856ee86fc52938c2f979b0f38e82000000004847304402204165be9a4cbab8049e1af9723b96199bfd3e85f44c6b4c0177e3962686b26073022028f638da23fc003760861ad481ead4099312c60030d4cb57820ce4d33812a5ce01ffffffff01009d966b01000000434104ea1feff861b51fe3f5f8a3b12d0f4712db80e919548a80839fc47c6a21e66d957e9c5d8cd108c7a2d2324bad71f9904ac0ae7336507d785b17a2c115e427a32fac00000000".from_hex().unwrap().iter().map(|n| *n)).unwrap();

    for tx in new_block.txdata.iter() {
      empty_set.add_utxos(tx);
    }

    let serial = empty_set.serialize();
    assert_eq!(serial, empty_set.serialize_iter().collect());

    let deserial: IoResult<UtxoSet> = Serializable::deserialize(serial.iter().map(|n| *n));
    assert!(deserial.is_ok());

    let mut read_set = deserial.unwrap();
    for tx in new_block.txdata.iter() {
      let hash = tx.hash();

      for (n, out) in tx.output.iter().enumerate() {
        let n = n as u32;
        // Try taking non-existent UTXO
        assert_eq!(read_set.take_utxo(hash, 100 + n), None);
        // Check take of real UTXO
        let ret = read_set.take_utxo(hash, n);
        assert_eq!(ret, Some(box out.clone()));
        // Try double-take
        assert_eq!(read_set.take_utxo(hash, n), None);
      }
    }
  }
}



