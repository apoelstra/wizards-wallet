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

//! # Blockdata constants
//!
//! This module provides various constants relating to the blockchain and
//! consensus code. In particular, it defines the genesis block and its
//! single transaction
//!

use blockdata::opcodes;
use blockdata::script::Script;
use blockdata::transaction::{Transaction, TxOut, TxIn};
use blockdata::block::{Block, BlockHeader};
use util::misc::hex_bytes;
use util::hash::{merkle_root, zero_hash};
use util::uint256::Uint256;

pub static MAX_SEQUENCE: u32 = 0xFFFFFFFF;
pub static COIN_VALUE: u64 = 100000000;
pub static DIFFCHANGE_INTERVAL: u32 = 2016;
pub static DIFFCHANGE_TIMESPAN: u32 = 14 * 24 * 3600;

/// In Bitcoind this is insanely described as ~((u256)0 >> 32)
pub fn max_target() -> Uint256 {
  Uint256::from_u64(0xFFFF).shl(208)
}

/// Constructs and returns the coinbase (and only) transaction of the genesis block
pub fn genesis_tx() -> Transaction {
  // Base
  let mut ret = Transaction {
    version: 1,
    lock_time: 0,
    input: vec![],
    output: vec![]
  };

  // Inputs
  let mut in_script = Script::new();
  in_script.push_scriptint(486604799);
  in_script.push_scriptint(4);
  in_script.push_slice("The Times 03/Jan/2009 Chancellor on brink of second bailout for banks".as_bytes());
  ret.input.push(TxIn {
    prev_hash: zero_hash(),
    prev_index: 0xFFFFFFFF,
    script_sig: in_script,
    sequence: MAX_SEQUENCE
  });

  // Outputs
  let mut out_script = Script::new();
  out_script.push_slice(hex_bytes("04678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5f").unwrap().as_slice());
  out_script.push_opcode(opcodes::CHECKSIG);
  ret.output.push(TxOut {
    value: 50 * COIN_VALUE,
    script_pubkey: out_script
  });

  // end
  ret
}

/// Constructs and returns the genesis block
pub fn genesis_block() -> Block {
  let txdata = vec![genesis_tx()];
  let header = BlockHeader {
    version: 1,
    prev_blockhash: zero_hash(),
    merkle_root: merkle_root(txdata.as_slice()),
    time: 1231006505,
    bits: 0x1d00ffff,
    nonce: 2083236893
  };

  Block {
    header: header,
    txdata: txdata
  }
}

#[cfg(test)]
mod test {
  use network::serialize::Serializable;
  use blockdata::constants::{genesis_block, genesis_tx};
  use blockdata::constants::{MAX_SEQUENCE, COIN_VALUE};
  use util::misc::hex_bytes;
  use util::hash::zero_hash;

  #[test]
  fn genesis_first_transaction() {
    let gen = genesis_tx();

    assert_eq!(gen.version, 1);
    assert_eq!(gen.input.len(), 1);
    assert_eq!(gen.input.get(0).prev_hash.as_slice(), zero_hash().as_slice());
    assert_eq!(gen.input.get(0).prev_index, 0xFFFFFFFF);
    assert_eq!(gen.input.get(0).script_sig.serialize().as_slice(),
               hex_bytes("4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73").unwrap().as_slice());

    assert_eq!(gen.input.get(0).sequence, MAX_SEQUENCE);
    assert_eq!(gen.output.len(), 1);
    assert_eq!(gen.output.get(0).script_pubkey.serialize().as_slice(),
               hex_bytes("434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac").unwrap().as_slice());
    assert_eq!(gen.output.get(0).value, 50 * COIN_VALUE);
    assert_eq!(gen.lock_time, 0);

    assert_eq!(gen.hash().serialize().iter().rev().map(|n| *n).collect::<Vec<u8>>(),
               hex_bytes("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b").unwrap());
  }

  #[test]
  fn genesis_full_block() {
    let gen = genesis_block();

    assert_eq!(gen.header.version, 1);
    assert_eq!(gen.header.prev_blockhash.as_slice(), zero_hash().as_slice());
    assert_eq!(gen.header.merkle_root.serialize().iter().rev().map(|n| *n).collect::<Vec<u8>>(),
               hex_bytes("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b").unwrap());
    assert_eq!(gen.header.time, 1231006505);
    assert_eq!(gen.header.bits, 0x1d00ffff);
    assert_eq!(gen.header.nonce, 2083236893);
    assert_eq!(gen.header.hash().serialize().iter().rev().map(|n| *n).collect::<Vec<u8>>(),
               hex_bytes("000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f").unwrap());
  }
}

