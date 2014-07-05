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

//! # Bitcoin Blockchain
//!
//! This module provides the structures and functions to maintain the
//! blockchain.
//!

use alloc::rc::Rc;
use std::cell::{Ref, RefCell};
use std::io::{IoResult, IoError, OtherIoError};

use blockdata::block::BlockHeader;
use blockdata::constants::{DIFFCHANGE_INTERVAL, DIFFCHANGE_TIMESPAN, max_target};
use network::serialize::{Serializable, SerializeIter};
use util::uint256::Uint256;
use util::hash::Sha256dHash;
use util::misc::prepend_err;
use util::patricia_tree::PatriciaTree;

/// A link in the blockchain
struct BlockchainNode {
  /// The blockheader
  header: BlockHeader,
  /// Total work from genesis to this point
  total_work: Uint256,
  /// Expected value of `header.bits` for this block; only changes every
  /// `blockdata::constants::DIFFCHANGE_INTERVAL;` blocks
  required_difficulty: Uint256,
  /// Height above genesis
  height: u32,
  /// Pointer to block's parent
  prev: RefCell<Option<Rc<BlockchainNode>>>,
  /// Pointer to block's child
  next: RefCell<Option<Rc<BlockchainNode>>>
}

impl BlockchainNode {
  /// Look up the previous link, caching the result
  fn prev(&self, tree: &PatriciaTree<Rc<BlockchainNode>>) -> Option<Rc<BlockchainNode>> {
    let mut cache = self.prev.borrow_mut();
    if cache.is_some() {
      return Some(cache.get_ref().clone())
    }
    match tree.lookup(&self.header.prev_blockhash.as_bitv()) {
      Some(prev) => { *cache = Some(prev.clone()); return Some(prev.clone()); }
      None => { return None; }
    }
  }

  /// Look up the next link
  fn next<'a>(&'a self) -> Ref<'a, Option<Rc<BlockchainNode>>> {
    self.next.borrow()
  }

  /// Set the next link
  fn set_next(&self, next: Rc<BlockchainNode>) {
    let mut cache = self.next.borrow_mut();
    *cache = Some(next);
  }
}

impl Serializable for Rc<BlockchainNode> {
  fn serialize(&self) -> Vec<u8> {
    let mut ret = vec![];
    ret.extend(self.header.serialize().move_iter());
    ret.extend(self.total_work.serialize().move_iter());
    ret.extend(self.required_difficulty.serialize().move_iter());
    ret.extend(self.height.serialize().move_iter());
    // Don't serialize the prev pointer
    ret
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<Rc<BlockchainNode>> {
    Ok(Rc::new(BlockchainNode {
      header: try!(prepend_err("header", Serializable::deserialize(iter.by_ref()))),
      total_work: try!(prepend_err("total_work", Serializable::deserialize(iter.by_ref()))),
      required_difficulty: try!(prepend_err("req_difficulty", Serializable::deserialize(iter.by_ref()))),
      height: try!(prepend_err("height", Serializable::deserialize(iter.by_ref()))),
      prev: RefCell::new(None),
      next: RefCell::new(None)
    }))
  }

  // Override Serialize::hash to return the blockheader hash, since the
  // hash of the node itself is pretty much meaningless.
  fn hash(&self) -> Sha256dHash {
    self.header.hash()
  }
}

/// The blockchain
pub struct Blockchain {
  tree: PatriciaTree<Rc<BlockchainNode>>,
  best_tip: Rc<BlockchainNode>,
  best_hash: Sha256dHash,
  genesis_hash: Sha256dHash
}

impl Serializable for Blockchain {
  fn serialize(&self) -> Vec<u8> {
    let mut ret = vec![];
    ret.extend(self.tree.serialize().move_iter());
    ret.extend(self.best_hash.serialize().move_iter());
    ret.extend(self.genesis_hash.serialize().move_iter());
    ret
  }

  fn serialize_iter<'a>(&'a self) -> SerializeIter<'a> {
    SerializeIter {
      data_iter: None,
      sub_iter_iter: box vec![ &self.tree as &Serializable,
                               &self.best_hash as &Serializable,
                               &self.genesis_hash as &Serializable ].move_iter(),
      sub_iter: None,
      sub_started: false
    }
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<Blockchain> {
    let tree: PatriciaTree<Rc<BlockchainNode>> = try!(prepend_err("tree", Serializable::deserialize(iter.by_ref())));
    let best_hash: Sha256dHash = try!(prepend_err("best_hash", Serializable::deserialize(iter.by_ref())));
    let genesis_hash: Sha256dHash = try!(prepend_err("genesis_hash", Serializable::deserialize(iter.by_ref())));
    // Lookup best tip
    let best = match tree.lookup(&best_hash.as_bitv()) {
      Some(rc) => rc.clone(),
      None => { return Err(IoError {
          kind: OtherIoError,
          desc: "best tip reference not found in tree",
          detail: Some(format!("best tip {:x} not found", best_hash))
        });
      }
    };
    // Lookup genesis
    if tree.lookup(&genesis_hash.as_bitv()).is_none() {
      return Err(IoError {
        kind: OtherIoError,
        desc: "genesis header not found in tree",
        detail: Some(format!("genesis header {:x} not found", genesis_hash))
      });
    }
    // Reconnect next and prev pointers back to "genesis", the first node
    // with no prev pointer.
    let mut scan = best.clone();
    let mut prev = best.prev(&tree);
    while prev.is_some() {
      prev.get_mut_ref().set_next(scan);
      scan = prev.get_ref().clone();
      prev = prev.get_ref().prev(&tree);
    }
    // Check that "genesis" is the genesis
    if scan.header.hash() != genesis_hash {
      Err(IoError {
          kind: OtherIoError,
          desc: "best tip did not link back to genesis",
          detail: Some(format!("no path from tip {:x} to genesis {:x}", best_hash, genesis_hash))
      })
    } else {
      // Return the chain
      Ok(Blockchain {
        tree: tree,
        best_tip: best.clone(),
        best_hash: best_hash,
        genesis_hash: genesis_hash
      })
    }
  }
}

struct LocatorHashIter<'tree> {
  index: Option<Rc<BlockchainNode>>,
  tree: &'tree PatriciaTree<Rc<BlockchainNode>>,
  count: uint,
  skip: uint
}

impl<'tree> LocatorHashIter<'tree> {
  fn new<'tree>(init: Rc<BlockchainNode>, tree: &'tree PatriciaTree<Rc<BlockchainNode>>) -> LocatorHashIter<'tree> {
    LocatorHashIter { index: Some(init), tree: tree, count: 0, skip: 1 }
  }
}

impl<'tree> Iterator<Sha256dHash> for LocatorHashIter<'tree> {
  fn next(&mut self) -> Option<Sha256dHash> {
    let ret = match self.index {
      Some(ref node) => Some(node.hash()),
      None => None
    };

    for _ in range(0, self.skip) {
      self.index = match self.index {
        Some(ref rc) => rc.prev(self.tree),
        None => { break; }
      }
    }

    self.count += 1;
    if self.count > 10 {
      self.skip *= 2;
    }
    ret
  }
}

/// This function emulates the GetCompact(SetCompact(n)) in the satoshi code,
/// which drops the precision to something that can be encoded precisely in
/// the nBits block header field. Savour the perversity. This is in Bitcoin
/// consensus code. What. The. Fuck.
fn satoshi_the_precision(n: &Uint256) -> Uint256 {
  // Shift by B bits right then left to turn the low bits to zero
  let bits = 8 * ((n.bits() + 7) / 8 - 3);
  let mut ret = n.shr(bits);
  // Oh, did I say B was that fucked up formula? I meant sometimes also + 8.
  if ret.bit_value(23) {
    ret = ret.shr(8).shl(8);
  }
  ret.shl(bits)
}

impl Blockchain {
  /// Constructs a new blockchain
  pub fn new(genesis: BlockHeader) -> Blockchain {
    let genhash = genesis.hash();
    let rc_gen = Rc::new(BlockchainNode {
      header: genesis,
      total_work: Uint256::from_u64(0),
      required_difficulty: genesis.target(),
      height: 0,
      prev: RefCell::new(None),
      next: RefCell::new(None)
    });
    Blockchain {
      tree: {
        let mut pat = PatriciaTree::new();
        pat.insert(&genhash.as_bitv(), rc_gen.clone());
        pat
      },
      best_hash: genhash,
      genesis_hash: genhash,
      best_tip: rc_gen,
    }
  }

  /// Adds a block header to the chain
  pub fn add_header(&mut self, header: BlockHeader) -> bool {
    // Construct node, if possible
    let rc_header = match self.tree.lookup(&header.prev_blockhash.as_bitv()) {
      Some(prev) => {
        let difficulty =
          // Compute required difficulty if this is a diffchange block
          if (prev.height + 1) % DIFFCHANGE_INTERVAL == 0 {
            // Scan back DIFFCHANGE_INTERVAL blocks
            let mut scan = prev.clone();
            for _ in range(0, DIFFCHANGE_INTERVAL - 1) {
              scan = scan.prev(&self.tree).unwrap();
            }
            // Get clamped timespan between first and last blocks
            let timespan = match prev.header.time - scan.header.time {
              n if n < DIFFCHANGE_TIMESPAN / 4 => DIFFCHANGE_TIMESPAN / 4,
              n if n > DIFFCHANGE_TIMESPAN * 4 => DIFFCHANGE_TIMESPAN * 4,
              n => n
            };
            // Compute new target
            let mut target = prev.header.target();
            target = target.mul_u32(timespan);
            target = target.div(&Uint256::from_u64(DIFFCHANGE_TIMESPAN as u64));
            // Clamp below MAX_TARGET (difficulty 1)
            let max = max_target();
            if target > max { target = max };
            // Compactify (make expressible in the 8+24 nBits float format
            satoshi_the_precision(&target)
          } else {
          // Otherwise just use the last block's difficulty
             prev.required_difficulty
          };
        // Create node
        let ret = Rc::new(BlockchainNode {
          header: header,
          total_work: header.work().add(&prev.total_work),
          required_difficulty: difficulty,
          height: prev.height + 1,
          prev: RefCell::new(Some(prev.clone())),
          next: RefCell::new(None)
        });
        prev.set_next(ret.clone());
        ret
      },
      None => {
        println!("TODO: couldn't add blockheader");
        return false;
      }
    };

    // spv validate the block
    if !header.spv_validate(&rc_header.required_difficulty) {
      return false;
    }

    // Insert the new block
    self.tree.insert(&header.hash().as_bitv(), rc_header.clone());
    // Replace the best tip if necessary
    if rc_header.total_work > self.best_tip.total_work {
      self.set_best_tip(rc_header);
    }
    return true;
  }

  /// Sets the best tip (not public)
  fn set_best_tip(&mut self, tip: Rc<BlockchainNode>) {
    let old_best = self.best_tip.clone();
    // Set best
    self.best_hash = tip.hash();
    self.best_tip = tip;
    // Fix next links
    let mut scan = self.best_tip.clone();
    let mut prev = self.best_tip.prev(&self.tree);
    // Scan backward
    loop {
      // If we hit the old best, there is no need to reorg
      if scan.header == old_best.header {
        break;
      }
      // If we hit the genesis, stop
      if prev.is_none() {
        println!("Warning: reorg past the genesis. This is a bug.");
        break;
      }
      // If we hit something pointing along the wrong chain, this is
      // a branch point at which we are reorg'ing
      if prev.get_ref().next().is_none() ||
         prev.get_ref().next().get_ref().header != scan.header {
        prev.get_mut_ref().set_next(scan);
      }
      scan = prev.clone().unwrap();
      prev = prev.unwrap().prev(&self.tree);
    }
  }

  /// Returns the best tip
  pub fn best_tip<'a>(&'a self) -> &'a BlockHeader {
    &self.best_tip.header
  }

  /// Returns an array of locator hashes used in `getheaders` messages
  pub fn locator_hashes(&self) -> Vec<Sha256dHash> {
    LocatorHashIter::new(self.best_tip.clone(), &self.tree).collect()
  }
}

#[cfg(test)]
mod tests {
  use std::prelude::*;
  use std::io::IoResult;

  use blockdata::blockchain::Blockchain;
  use blockdata::constants::genesis_block;
  use network::serialize::Serializable;

  #[test]
  fn blockchain_serialize_test() {
    let empty_chain = Blockchain::new(genesis_block().header);
    assert_eq!(empty_chain.best_tip.hash().serialize(), genesis_block().header.hash().serialize());

    let serial = empty_chain.serialize();
    assert_eq!(serial, empty_chain.serialize_iter().collect());

    let deserial: IoResult<Blockchain> = Serializable::deserialize(serial.iter().map(|n| *n));
    assert!(deserial.is_ok());
    let read_chain = deserial.unwrap();
    assert_eq!(read_chain.best_tip.hash().serialize(), genesis_block().header.hash().serialize());
  }
}



