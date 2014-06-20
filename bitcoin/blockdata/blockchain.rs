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
use std::cell::RefCell;
use std::io::{IoResult, IoError, OtherIoError};

use blockdata::block::BlockHeader;
use blockdata::constants::{DIFFCHANGE_INTERVAL, DIFFCHANGE_TIMESPAN, max_target};
use network::serialize::Serializable;
use util::uint256::Uint256;
use util::hash::Sha256dHash;
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
  prev: RefCell<Option<Rc<BlockchainNode>>>
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
      header: try!(Serializable::deserialize(iter.by_ref())),
      total_work: try!(Serializable::deserialize(iter.by_ref())),
      required_difficulty: try!(Serializable::deserialize(iter.by_ref())),
      height: try!(Serializable::deserialize(iter.by_ref())),
      prev: RefCell::new(None)
    }))
  }
}

/// The blockchain
pub struct Blockchain {
  tree: PatriciaTree<Rc<BlockchainNode>>,
  best_tip: Rc<BlockchainNode>
}

impl Serializable for Blockchain {
  fn serialize(&self) -> Vec<u8> {
    let mut ret = vec![];
    ret.extend(self.tree.serialize().move_iter());
    ret.extend(self.best_tip.header.hash().serialize().move_iter());
    ret
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<Blockchain> {
    let tree: PatriciaTree<Rc<BlockchainNode>> = try!(Serializable::deserialize(iter.by_ref()));
    let hash: Sha256dHash = try!(Serializable::deserialize(iter.by_ref()));
    let best = match tree.lookup(&hash.as_bitv()) {
      Some(rc) => rc.clone(),
      None => { return Err(IoError {
          kind: OtherIoError,
          desc: "best tip reference not found in tree",
          detail: None
        });
      }
    };
    Ok(Blockchain {
      tree: tree,
      best_tip: best.clone()
    })
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
      Some(ref node) => Some(node.header.hash()),
      None => None
    };

    for _ in range(0, self.skip) {
      self.index = match self.index {
        Some(ref rc) => rc.prev(self.tree),
        None => None
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
    let genhash = genesis.hash().as_bitv();
    let rc_gen = Rc::new(BlockchainNode {
      header: genesis,
      total_work: Uint256::from_u64(0),
      required_difficulty: genesis.target(),
      height: 0,
      prev: RefCell::new(None)
    });
    Blockchain {
      tree: {
        let mut pat = PatriciaTree::new();
        pat.insert(&genhash, rc_gen.clone());
        pat
      },
      best_tip: rc_gen
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
        Rc::new(BlockchainNode {
          header: header,
          total_work: header.work().add(&prev.total_work),
          required_difficulty: difficulty,
          height: prev.height + 1,
          prev: RefCell::new(Some(prev.clone()))
        })
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
      self.best_tip = rc_header;
    }
    return true;
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





