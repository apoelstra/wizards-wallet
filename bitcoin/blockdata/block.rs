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

pub struct BlockHeader {
  version: u32,
  prev_blockhash: Sha256dHash,
  merkle_root: Sha256dHash,
  time: u32,
  bits: u32,
  nonce: u32
}

pub struct Block {
  pub header: BlockHeader
}

impl Serializable for BlockHeader {
  fn serialize(&self) -> Vec<u8> {
    let mut ret = vec![];
    ret.extend(self.version.serialize().move_iter());
    ret.extend(self.prev_blockhash.serialize().move_iter());
    ret.extend(self.merkle_root.serialize().move_iter());
    ret.extend(self.time.serialize().move_iter());
    ret.extend(self.bits.serialize().move_iter());
    ret.extend(self.nonce.serialize().move_iter());
    ret
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<BlockHeader> {
    Ok(BlockHeader {
      version: try!(Serializable::deserialize(iter.by_ref())),
      prev_blockhash: try!(Serializable::deserialize(iter.by_ref())),
      merkle_root: try!(Serializable::deserialize(iter.by_ref())),
      time: try!(Serializable::deserialize(iter.by_ref())),
      bits: try!(Serializable::deserialize(iter.by_ref())),
      nonce: try!(Serializable::deserialize(iter.by_ref()))
    })
  }
}



