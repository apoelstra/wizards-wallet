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

//! # Bitcoin Transaction
//!
//! A transaction describes a transfer of money. It consumes previously-unspent
//! transaction outputs and produces new ones, satisfying the condition to spend
//! the old outputs (typically a digital signature with a specific key must be
//! provided) and defining the condition to spend the new ones. The use of digital
//! signatures ensures that coins cannot be spent by unauthorized parties.
//!
//! This module provides the structures and functions needed to support transactions.
//!

use std::io::IoResult;
use util::hash::Sha256dHash;
use network::serialize::{Serializable, SerializeIter};
use blockdata::script::Script;
#[cfg(test)]
use util::misc::hex_bytes;

/// A transaction input, which defines old coins to be consumed
#[deriving(Clone, PartialEq, Show)]
pub struct TxIn {
  /// The hash of the transaction whose output is being used an an input
  pub prev_hash: Sha256dHash,
  /// The index of the output in the previous transaction, which may have several
  pub prev_index: u32,
  /// The script which pushes values on the stack which will cause
  /// the referenced output's script to accept
  pub script_sig: Script,
  /// The sequence number, which suggests to miners which of two
  /// conflicting transactions should be preferred, or 0xFFFFFFFF
  /// to ignore this feature. This is generally never used since
  /// the miner behaviour cannot be enforced.
  pub sequence: u32,
}

/// A transaction output, which defines new coins to be created from old ones.
#[deriving(Clone, PartialEq, Show)]
pub struct TxOut {
  /// The value of the output, in satoshis
  pub value: u64,
  /// The script which must satisfy for the output to be spent
  pub script_pubkey: Script
}

/// A Bitcoin transaction, which describes an authenticated movement of coins
#[deriving(Clone, PartialEq, Show)]
pub struct Transaction {
  /// The protocol version, should always be 1.
  pub version: u32,
  /// Block number before which this transaction is valid, or 0 for
  /// valid immediately.
  pub lock_time: u32,
  /// List of inputs
  pub input: Vec<TxIn>,
  /// List of outputs
  pub output: Vec<TxOut>
}

impl_serializable!(TxIn, prev_hash, prev_index, script_sig, sequence)
impl_serializable!(TxOut, value, script_pubkey)
impl_serializable!(Transaction, version, input, output, lock_time)

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
  // as little-endian 256-bit numbers rather than as data strings. (This is why we
  // have this crazy .iter().rev() thing going on in many hash-related tests.
  assert_eq!(realtx.input.get(0).prev_hash.as_slice().iter().rev().map(|n| *n).collect::<Vec<u8>>(),
             hex_bytes("ce9ea9f6f5e422c6a9dbcddb3b9a14d1c78fab9ab520cb281aa2a74a09575da1").unwrap());
  assert_eq!(realtx.input.get(0).prev_index, 1);
  assert_eq!(realtx.output.len(), 1);
  assert_eq!(realtx.lock_time, 0);

  assert_eq!(realtx.hash().serialize().iter().rev().map(|n| *n).collect::<Vec<u8>>(),
             hex_bytes("a6eab3c14ab5272a58a5ba91505ba1a4b6d7a3a9fcbd187b6cd99a7b6d548cb7").unwrap());
}



