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

//! # Blockdata network messages
//!
//! This module describes network messages which are used for passing
//! Bitcoin data (blocks and transactions) around.
//!

use std::io::{IoResult, IoError, InvalidInput};
#[cfg(test)]
use serialize::hex::FromHex;
#[cfg(test)]
use util::hash::zero_hash;

use blockdata::block::{Block, LoneBlockHeader};
use network::constants;
use network::serialize::Message;
use network::serialize::{Serializable, SerializeIter};
use util::hash::Sha256dHash;

#[deriving(PartialEq, Show)]
/// The type of an inventory object
pub enum InvType {
  /// Error --- these inventories can be ignored
  InvError,
  /// Transaction
  InvTransaction,
  /// Block
  InvBlock
}

// Some simple messages

/// The `getblocks` message
pub struct GetBlocksMessage {
  /// The protocol version
  pub version: u32,
  /// Locator hashes --- ordered newest to oldest. The remote peer will
  /// reply with its longest known chain, starting from a locator hash
  /// if possible and block 1 otherwise.
  pub locator_hashes: Vec<Sha256dHash>,
  /// References the block to stop at, or zero to just fetch the maximum 500 blocks
  pub stop_hash: Sha256dHash
}

/// The `getheaders` message
pub struct GetHeadersMessage {
  /// The protocol version
  pub version: u32,
  /// Locator hashes --- ordered newest to oldest. The remote peer will
  /// reply with its longest known chain, starting from a locator hash
  /// if possible and block 1 otherwise.
  pub locator_hashes: Vec<Sha256dHash>,
  /// References the header to stop at, or zero to just fetch the maximum 2000 headers
  pub stop_hash: Sha256dHash
}

/// An inventory object --- a reference to a Bitcoin object
pub struct Inventory {
  /// The type of object that is referenced
  pub inv_type: InvType,
  /// The object's hash
  pub hash: Sha256dHash
}

/// The `inv` message
pub struct InventoryMessage(pub Vec<Inventory>);

/// The `getdata` message
pub struct GetDataMessage(pub Vec<Inventory>);

/// The `notfound` message
pub struct NotFoundMessage(pub Vec<Inventory>);

/// The `headers` message
pub struct HeadersMessage(pub Vec<LoneBlockHeader>);

// The block message is literally just a block
/// The `block` message
type BlockMessage = Block;
impl Message for BlockMessage {
  fn command(&self) -> String { String::from_str("block") }
}

impl GetBlocksMessage {
  /// Construct a new `getblocks` message
  pub fn new(locator_hashes: Vec<Sha256dHash>, stop_hash: Sha256dHash) -> GetBlocksMessage {
    GetBlocksMessage {
      version: constants::PROTOCOL_VERSION,
      locator_hashes: locator_hashes.clone(),
      stop_hash: stop_hash
    }
  }
}

impl_serializable!(GetBlocksMessage, version, locator_hashes, stop_hash)
impl_message!(GetBlocksMessage, "getblocks")

impl GetHeadersMessage {
  /// Construct a new `getheaders` message
  pub fn new(locator_hashes: Vec<Sha256dHash>, stop_hash: Sha256dHash) -> GetHeadersMessage {
    GetHeadersMessage {
      version: constants::PROTOCOL_VERSION,
      locator_hashes: locator_hashes.clone(),
      stop_hash: stop_hash
    }
  }
}

impl_serializable!(GetHeadersMessage, version, locator_hashes, stop_hash)
impl_message!(GetHeadersMessage, "getheaders")

impl Serializable for Inventory {
  fn serialize(&self) -> Vec<u8> {
    let int_type: u32 = match self.inv_type {
      InvError => 0, 
      InvTransaction => 1,
      InvBlock => 2
    };
    let mut rv = vec!();
    rv.extend(int_type.serialize().move_iter());
    rv.extend(self.hash.serialize().move_iter());
    rv
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<Inventory> {
    let int_type: u32 = try!(Serializable::deserialize(iter.by_ref()));
    Ok(Inventory {
      inv_type: match int_type {
        0 => InvError,
        1 => InvTransaction,
        2 => InvBlock,
        _ => { return Err(IoError {
          kind: InvalidInput,
          desc: "bad inventory type field",
          detail: None
        })}
      },
      hash: try!(Serializable::deserialize(iter.by_ref()))
    })
  }
}

impl_serializable_newtype!(InventoryMessage, Vec<Inventory>)
impl_message!(InventoryMessage, "inv")

impl_serializable_newtype!(GetDataMessage, Vec<Inventory>)
impl_message!(GetDataMessage, "getdata")

impl_serializable_newtype!(NotFoundMessage, Vec<Inventory>)
impl_message!(NotFoundMessage, "notfound")

impl_serializable_newtype!(HeadersMessage, Vec<LoneBlockHeader>)
impl_message!(HeadersMessage, "headers")

#[test]
fn getblocks_message_test() {
  let from_sat = "72110100014a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b0000000000000000000000000000000000000000000000000000000000000000".from_hex().unwrap();
  let genhash = "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b".from_hex().unwrap();

  let decode: IoResult<GetBlocksMessage> = Serializable::deserialize(from_sat.iter().map(|n| *n));
  assert!(decode.is_ok());
  let real_decode = decode.unwrap();
  assert_eq!(real_decode.version, 70002);
  assert_eq!(real_decode.locator_hashes.len(), 1);
  assert_eq!(real_decode.locator_hashes.get(0).as_slice(), genhash.as_slice());
  assert_eq!(real_decode.stop_hash.as_slice(), zero_hash().as_slice());

  let reserialize = real_decode.serialize();
  assert_eq!(reserialize.as_slice(), from_sat.as_slice());
}

#[test]
fn getheaders_message_test() {
  let from_sat = "72110100014a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b0000000000000000000000000000000000000000000000000000000000000000".from_hex().unwrap();
  let genhash = "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b".from_hex().unwrap();

  let decode: IoResult<GetHeadersMessage> = Serializable::deserialize(from_sat.iter().map(|n| *n));
  assert!(decode.is_ok());
  let real_decode = decode.unwrap();
  assert_eq!(real_decode.version, 70002);
  assert_eq!(real_decode.locator_hashes.len(), 1);
  assert_eq!(real_decode.locator_hashes.get(0).as_slice(), genhash.as_slice());
  assert_eq!(real_decode.stop_hash.as_slice(), zero_hash().as_slice());

  let reserialize = real_decode.serialize();
  assert_eq!(reserialize.as_slice(), from_sat.as_slice());
}


#[test]
fn inv_message_test() {
  // I originally had the first 500 here, but vim gets irritated by 36k lines..
  let first_20 = "14020000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a830000000002000000bddd99ccfda39da1b108ce1a5d70038d0a967bacb68b6b63065f626a00000000020000004944469562ae1c2c74d9a535e00b6f3e40ffbad4f2fda3895501b582000000000200000085144a84488ea88d221c8bd6c059da090e88f8a2c99690ee55dbba4e0000000002000000fc33f596f822a0a1951ffdbf2a897b095636ad871707bf5d3162729b00000000020000008d778fdc15a2d3fb76b7122a3b5582bea4f21f5a0c693537e7a0313000000000020000004494c8cf4154bdcc0720cd4a59d9c9b285e4b146d45f061d2b6c96710000000002000000c60ddef1b7618ca2348a46e868afc26e3efc68226c78aa47f8488c4000000000020000000508085c47cc849eb80ea905cc7800a3be674ffc57263cf210c59d8d0000000002000000e915d9a478e3adf3186c07c61a22228b10fd87df343c92782ecc052c00000000020000007330d7adf261c69891e6ab08367d957e74d4044bc5d9cd06d656be9700000000020000005e2b8043bd9f8db558c284e00ea24f78879736f4acd110258e48c227000000000200000089304d4ba5542a22fb616d1ca019e94222ee45c1ad95a83120de515c0000000002000000378a6f6593e2f0251132d96616e837eb6999bca963f6675a0c7af18000000000020000007384231257343f2fa3c55ee69ea9e676a709a06dcfd2f73e8c2c32b30000000002000000f5c46c41c30df6aaff3ae9f74da83e4b1cffdec89c009b39bb254a17000000000200000009f8fd6ba6f0b6d5c207e8fcbcf50f46876a5deffbac4701d7d0f13f0000000002000000161126f0d39ec082e51bbd29a1dfb40b416b445ac8e493f88ce9938600000000020000006f187fddd5e28aa1b4065daa5d9eae0c487094fb20cf97ca02b81c840000000002000000d7c834e8ea05e2c2fddf4d82faf4c3e921027fa190f1b8372a7aa96700000000".from_hex().unwrap();

  let firsthash = "4860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000".from_hex().unwrap();
  let lasthash = "d7c834e8ea05e2c2fddf4d82faf4c3e921027fa190f1b8372a7aa96700000000".from_hex().unwrap();

  let decode1: IoResult<InventoryMessage> = Serializable::deserialize(first_20.iter().map(|n| *n));
  let decode2: IoResult<GetDataMessage> = Serializable::deserialize(first_20.iter().map(|n| *n));
  let decode3: IoResult<NotFoundMessage> = Serializable::deserialize(first_20.iter().map(|n| *n));

  assert!(decode1.is_ok());
  assert!(decode2.is_ok());
  assert!(decode3.is_ok());
  let InventoryMessage(real_decode1) = decode1.unwrap();
  let GetDataMessage(real_decode2) = decode2.unwrap();
  let NotFoundMessage(real_decode3) = decode3.unwrap();
  assert_eq!(real_decode1.len(), 20);
  assert_eq!(real_decode2.len(), 20);
  assert_eq!(real_decode3.len(), 20);
  assert_eq!(real_decode1.get(0).inv_type, InvBlock);
  assert_eq!(real_decode2.get(0).inv_type, InvBlock);
  assert_eq!(real_decode3.get(0).inv_type, InvBlock);
  assert_eq!(real_decode1.get(0).hash.as_slice(), firsthash.as_slice());
  assert_eq!(real_decode2.get(0).hash.as_slice(), firsthash.as_slice());
  assert_eq!(real_decode3.get(0).hash.as_slice(), firsthash.as_slice());
  assert_eq!(real_decode1.get(19).inv_type, InvBlock);
  assert_eq!(real_decode2.get(19).inv_type, InvBlock);
  assert_eq!(real_decode3.get(19).inv_type, InvBlock);
  assert_eq!(real_decode1.get(19).hash.as_slice(), lasthash.as_slice());
  assert_eq!(real_decode2.get(19).hash.as_slice(), lasthash.as_slice());
  assert_eq!(real_decode3.get(19).hash.as_slice(), lasthash.as_slice());
  
  let reserialize1 = InventoryMessage(real_decode1).serialize();
  let reserialize2 = GetDataMessage(real_decode2).serialize();
  let reserialize3 = NotFoundMessage(real_decode3).serialize();
  assert_eq!(reserialize1.as_slice(), first_20.as_slice());
  assert_eq!(reserialize2.as_slice(), first_20.as_slice());
  assert_eq!(reserialize3.as_slice(), first_20.as_slice());
}

