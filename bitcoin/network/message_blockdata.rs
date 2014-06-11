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

use std::io::IoResult;
#[cfg(test)]
use serialize::hex::FromHex;
#[cfg(test)]
use util::hash::zero_hash;

use network::constants;
use network::serialize::CommandString;
use network::serialize::Message;
use network::serialize::Serializable;
use util::hash::Sha256dHash;

/// Some simple messages
pub struct GetBlocksMessage {
  pub version: u32,
  pub locator_hashes: Vec<Sha256dHash>,
  pub stop_hash: Sha256dHash
}

impl GetBlocksMessage {
  // TODO: we have fixed services and relay to 0
  pub fn new(locator_hashes: Vec<Sha256dHash>, stop_hash: Sha256dHash) -> GetBlocksMessage {
    GetBlocksMessage {
      version: constants::PROTOCOL_VERSION,
      locator_hashes: locator_hashes.clone(),
      stop_hash: stop_hash
    }
  }

  fn command() -> CommandString {
    CommandString::new("getblocks")
  }
}

impl Serializable for GetBlocksMessage {
  fn serialize(&self) -> Vec<u8> {
    let mut rv = vec!();
    rv.extend(self.version.serialize().move_iter());
    rv.extend(self.locator_hashes.serialize().move_iter());
    rv.extend(self.stop_hash.serialize().move_iter());
    rv
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<GetBlocksMessage> {
    Ok(GetBlocksMessage {
      version: try!(Serializable::deserialize(iter.by_ref())),
      locator_hashes: try!(Serializable::deserialize(iter.by_ref())),
      stop_hash: try!(Serializable::deserialize(iter.by_ref()))
    })
  }
}

impl Message for GetBlocksMessage {
  fn command(&self) -> CommandString {
    GetBlocksMessage::command()
  }
}


#[test]
fn getblocks_message_test() {
  let from_sat = "72110100014a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b0000000000000000000000000000000000000000000000000000000000000000".from_hex().unwrap();
  let mut genhash = "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b".from_hex().unwrap();
  genhash.reverse();

  let decode: IoResult<GetBlocksMessage> = Serializable::deserialize(from_sat.iter().map(|n| *n));
  assert!(decode.is_ok());
  let real_decode = decode.unwrap();
  assert_eq!(real_decode.version, 70002);
  assert_eq!(real_decode.locator_hashes.len(), 1);
  assert_eq!(real_decode.locator_hashes.get(0).data(), genhash.as_slice());
  assert_eq!(real_decode.stop_hash.data(), zero_hash().data());

  let reserialize = real_decode.serialize();
  assert_eq!(reserialize.as_slice(), from_sat.as_slice());
}



