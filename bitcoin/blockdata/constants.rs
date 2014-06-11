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

use blockdata::opcodes;
use blockdata::script::Script;
use blockdata::transaction::{Transaction, TxOut, TxIn};
use util::misc::hex_bytes;
use util::hash::zero_hash;
#[cfg(test)]
use network::serialize::Serializable;

pub static MAX_SEQUENCE: u32 = 0xFFFFFFFF;
pub static COIN_VALUE: u64 = 100000000;

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

#[test]
fn test_genesis() {
  let gen = genesis_tx();

  assert_eq!(gen.version, 1);
  assert_eq!(gen.input.len(), 1);
  assert_eq!(gen.input.get(0).prev_hash.data(), zero_hash().data());
  assert_eq!(gen.input.get(0).prev_index, 0xFFFFFFFF);
  assert_eq!(gen.input.get(0).script_sig.serialize().as_slice(),
             hex_bytes("4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73").unwrap().as_slice());
                        
  assert_eq!(gen.input.get(0).sequence, MAX_SEQUENCE);
  assert_eq!(gen.output.len(), 1);
  assert_eq!(gen.output.get(0).script_pubkey.serialize().as_slice(),
             hex_bytes("434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac").unwrap().as_slice());
  assert_eq!(gen.output.get(0).value, 50 * COIN_VALUE);
  assert_eq!(gen.lock_time, 0);

  assert_eq!(gen.hash().serialize(),
             hex_bytes("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b").unwrap());
}


