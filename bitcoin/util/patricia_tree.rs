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

//! # Patricia/Radix Trie 
//!
//! A Patricia trie is a trie in which nodes with only one child are
//! merged with the child, giving huge space savings for sparse tries.
//! A radix tree is more general, working with keys that are arbitrary
//! strings; a Patricia tree uses bitstrings.
//!

use core::fmt::Show;
use core::iter::ByRef;
use core::cmp;
use std::num::Zero;
use std::io::{IoResult, InvalidInput, standard_error};

use network::serialize::{Serializable, SerializeIter};
use util::uint256::Uint256;
use util::misc::prepend_err;

/// Patricia troo
pub struct PatriciaTree<T> {
  data: Option<T>,
  child_l: Option<Box<PatriciaTree<T>>>,
  child_r: Option<Box<PatriciaTree<T>>>,
  skip_prefix: Uint256,
  skip_len: u8
}

impl<T> PatriciaTree<T> {
  /// Constructs a new Patricia tree
  pub fn new() -> PatriciaTree<T> {
    PatriciaTree {
      data: None,
      child_l: None,
      child_r: None,
      skip_prefix: Zero::zero(),
      skip_len: 0
    }
  }

  /// Lookup a value by exactly matching `key` and return a referenc
  pub fn lookup_mut<'a>(&'a mut self, key: &Uint256, key_len: uint) -> Option<&'a mut T> {
    // Caution: `lookup_mut` never modifies its self parameter (in fact its
    // internal recursion uses a non-mutable self, so we are OK to just
    // transmute our self pointer into a mutable self before passing it in.
    use std::mem::transmute;
    unsafe { transmute(self.lookup(key, key_len)) }
  }

  /// Lookup a value by exactly matching `key` and return a mutable reference
  pub fn lookup<'a>(&'a self, key: &Uint256, key_len: uint) -> Option<&'a T> {
    let mut node = self;
    let mut key_idx = 0;

    loop {
      // If the search key is shorter than the node prefix, there is no
      // way we can match, so fail.
      if key_len - key_idx < node.skip_len as uint {
        return None;
      }

      // Key fails to match prefix --- no match
      if node.skip_prefix != key.bit_slice(key_idx, key_idx + node.skip_len as uint) {
        return None;
      }

      // Key matches prefix: if they are an exact match, return the data
      if node.skip_len as uint == key_len - key_idx {
        return node.data.as_ref();
      } else {
        // Key matches prefix: search key longer than node key, recurse
        key_idx += 1 + node.skip_len as uint;
        let subtree = if key.bit_value(key_idx - 1) { &node.child_r } else { &node.child_l };
        match subtree {
          &Some(ref bx) => {
            node = &**bx;  // bx is a &Box<U> here, so &**bx gets &U
          }
          &None => { return None; }
        }
      }
    } // end loop
  }

  /// Inserts a value with key `key`, returning true on success. If a value is already
  /// stored against `key`, do nothing and return false.
  pub fn insert(&mut self, key: &Uint256, key_len: uint, value: T) -> bool {
    let mut node = self;
    let mut idx = 0;
    loop {
      // Mask in case search key is shorter than node key
      let slice_len = cmp::min(node.skip_len as uint, key_len - idx);
      let masked_prefix = node.skip_prefix.mask(slice_len);
      let key_slice = key.bit_slice(idx, idx + slice_len);

      // Prefixes do not match: split key
      if masked_prefix != key_slice {
        let diff = masked_prefix.xor(&key_slice).trailing_zeros();

        // Remove the old node's children
        let child_l = node.child_l.take();
        let child_r = node.child_r.take();
        let value_neighbor = node.data.take();
        let tmp = node;  // borrowck hack
        let (insert, neighbor) = if key_slice.bit_value(diff)
                                      { (&mut tmp.child_r, &mut tmp.child_l) }
                                 else { (&mut tmp.child_l, &mut tmp.child_r) };
        *insert = Some(box PatriciaTree {
          data: None,
          child_l: None,
          child_r: None,
          skip_prefix: key.bit_slice(idx + diff + 1, key_len),
          skip_len: (key_len - idx - diff - 1) as u8
        });
        *neighbor = Some(box PatriciaTree {
          data: value_neighbor,
          child_l: child_l,
          child_r: child_r,
          skip_prefix: tmp.skip_prefix.shr(diff + 1),
          skip_len: tmp.skip_len - diff as u8 - 1
        });
        // Chop the prefix down
        tmp.skip_len = diff as u8;
        tmp.skip_prefix = tmp.skip_prefix.mask(diff);
        // Recurse
        idx += 1 + diff;
        node = &mut **insert.get_mut_ref();
      }
      // Prefixes match
      else {
        let slice_len = key_len - idx;
        // Search key is shorter than skip prefix: truncate the prefix and attach
        // the old data as a child
        if node.skip_len as uint > slice_len {
          // Remove the old node's children
          let child_l = node.child_l.take();
          let child_r = node.child_r.take();
          let value_neighbor = node.data.take();
          // Put the old data in a new child, with the remainder of the prefix
          let new_child = if node.skip_prefix.bit_value(slice_len)
                            { &mut node.child_r } else { &mut node.child_l };
          *new_child = Some(box PatriciaTree {
            data: value_neighbor,
            child_l: child_l,
            child_r: child_r,
            skip_prefix: node.skip_prefix.shr(slice_len + 1),
            skip_len: node.skip_len - slice_len as u8 - 1
          });
          // Chop the prefix down and put the new data in place
          node.skip_len = slice_len as u8;
          node.skip_prefix = key_slice;
          node.data = Some(value);
          return true;
        }
        // If we have an exact match, great, insert it
        else if node.skip_len as uint == slice_len {
          if node.data.is_none() {
            node.data = Some(value);
            return true;
          }
          return false;
        }
        // Search key longer than node key, recurse
        else {
          let tmp = node;  // hack to appease borrowck
          idx += tmp.skip_len as uint + 1;
          let subtree = if key.bit_value(idx - 1)
                          { &mut tmp.child_r } else { &mut tmp.child_l };
          // Recurse, adding a new node if necessary
          if subtree.is_none() {
            *subtree = Some(box PatriciaTree {
              data: None,
              child_l: None,
              child_r: None,
              skip_prefix: key.bit_slice(idx, key_len),
              skip_len: key_len as u8 - idx as u8
            });
          }
          // subtree.get_mut_ref is a &mut Box<U> here, so &mut ** gets a &mut U
          node = &mut **subtree.get_mut_ref();
        } // end search_len vs prefix len
      } // end if prefixes match
    } // end loop
  }

  /// Deletes a value with key `key`, returning it on success. If no value with
  /// the given key is found, return None
  pub fn delete(&mut self, key: &Uint256, key_len: uint) -> Option<T> {
    /// Return value is (deletable, actual return value), where `deletable` is true
    /// is true when the entire node can be deleted (i.e. it has no children)
    fn recurse<T>(tree: &mut PatriciaTree<T>, key: Uint256, key_len: uint) -> (bool, Option<T>) {
      // If the search key is shorter than the node prefix, there is no
      // way we can match, so fail.
      if key_len < tree.skip_len as uint {
        return (false, None);
      }

      // Key fails to match prefix --- no match
      if tree.skip_prefix != key.mask(tree.skip_len as uint) {
        return (false, None);
      }

      // If we are here, the key matches the prefix
      if tree.skip_len as uint == key_len {
        // Exact match -- delete and return
        let ret = tree.data.take();
        let bit = tree.child_r.is_some();
        // First try to consolidate if there is only one child
        if tree.child_l.is_some() && tree.child_r.is_some() {
          // Two children means we cannot consolidate or delete
          return (false, ret);
        }
        match (tree.child_l.take(), tree.child_r.take()) {
          (Some(_), Some(_)) => unreachable!(),
          (Some(consolidate), None) | (None, Some(consolidate)) => {
            tree.data = consolidate.data;
            tree.child_l = consolidate.child_l;
            tree.child_r = consolidate.child_r;
            let new_bit = if bit { Uint256::from_u64(1).shl(tree.skip_len as uint) }
                          else { Zero::zero() };
            tree.skip_prefix = tree.skip_prefix.add(&new_bit)
                                               .add(&consolidate.skip_prefix.shl(1 + tree.skip_len as uint));
            tree.skip_len += 1 + consolidate.skip_len;
            return (false, ret);
          }
          // No children means this node is deletable
          (None, None) => { return (true, ret); }
        }
      }

      // Otherwise, the key is longer than the prefix and we need to recurse
      let next_bit = key.bit_value(tree.skip_len as uint);
      // Recursively get the return value. This awkward scope is required
      // to shorten the time we mutably borrow the node's children -- we
      // might want to borrow the sibling later, so the borrow needs to end.
      let ret = {
        let target = if next_bit { &mut tree.child_r } else { &mut tree.child_l };

        // If we can't recurse, fail
        if target.is_none() {
          return (false, None);
        }
        // Otherwise, do it
        let (delete_child, ret) = recurse(&mut **target.get_mut_ref(),
                                          key.shr(tree.skip_len as uint + 1),
                                          key_len - tree.skip_len as uint - 1);
        if delete_child {
          target.take();
        }
        ret
      };

      // The above block may have deleted the target. If we now have only one
      // child, merge it into the parent. (If we have no children, mark this
      // node for deletion.)
      if tree.data.is_some() {
        // First though, if this is a data node, we can neither delete nor
        // consolidate it.
        return (false, ret);
      }

      match (tree.child_r.is_some(), tree.child_l.take(), tree.child_r.take()) {
        // Two children? Can't do anything, just sheepishly put them back
        (_, Some(child_l), Some(child_r)) => {
          tree.child_l = Some(child_l);
          tree.child_r = Some(child_r);
          return (false, ret);
        }
        // One child? Consolidate
        (bit, Some(consolidate), None) | (bit, None, Some(consolidate)) => {
          tree.data = consolidate.data;
          tree.child_l = consolidate.child_l;
          tree.child_r = consolidate.child_r;
          let new_bit = if bit { Uint256::from_u64(1).shl(tree.skip_len as uint) }
                        else { Zero::zero() };
          tree.skip_prefix = tree.skip_prefix.add(&new_bit)
                                             .add(&consolidate.skip_prefix.shl(1 + tree.skip_len as uint));
          tree.skip_len += 1 + consolidate.skip_len;
          return (false, ret);
        }
        // No children? Delete
        (_, None, None) => {
          return (true, ret);
        }
      }
    }
    let (_, ret) = recurse(self, *key, key_len);
    ret
  }
}

impl<T:Show> PatriciaTree<T> {
  /// Print the entire tree
  pub fn print<'a>(&'a self) {
    fn recurse<'a, T:Show>(tree: &'a PatriciaTree<T>, depth: uint) {
      for i in range(0, tree.skip_len as uint) {
        print!("{:}", if tree.skip_prefix.bit_value(i) { 1u } else { 0 });
      }
      println!(": {:}", tree.data);
      // left gets no indentation
      match tree.child_l {
        Some(ref t) => {
          for _ in range(0, depth + tree.skip_len as uint) {
            print!("-");
          }
          print!("0");
          recurse(&**t, depth + tree.skip_len as uint + 1);
        }
        None => { }
      }
      // right one gets indentation
      match tree.child_r {
        Some(ref t) => {
          for _ in range(0, depth + tree.skip_len as uint) {
            print!("_");
          }
          print!("1");
          recurse(&**t, depth + tree.skip_len as uint + 1);
        }
        None => { }
      }
    }
    recurse(self, 0);
  }
}

impl<T:Serializable+'static> Serializable for PatriciaTree<T> {
  fn serialize(&self) -> Vec<u8> {
    // Depth-first serialization
    let mut ret = vec![];
    // Serialize self, then children
    ret.extend(self.skip_prefix.serialize().move_iter());
    ret.extend(self.skip_len.serialize().move_iter());
    ret.extend(self.data.serialize().move_iter());
    ret.extend(self.child_l.serialize().move_iter());
    ret.extend(self.child_r.serialize().move_iter());
    ret
  }

  fn serialize_iter<'a>(&'a self) -> SerializeIter<'a> {
    SerializeIter {
      data_iter: None,
      sub_iter_iter: box vec![ &self.skip_prefix as &Serializable,
                               &self.skip_len as &Serializable,
                               &self.data as &Serializable,
                               &self.child_l as &Serializable,
                               &self.child_r as &Serializable ].move_iter(),
      sub_iter: None,
      sub_started: false
    }
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<PatriciaTree<T>> {
    // This goofy deserialization routine is to prevent an infinite
    // regress of ByRef<ByRef<...<ByRef<I>>...>>, see #15188
    fn recurse<T:Serializable, I: Iterator<u8>>(iter: &mut ByRef<I>) -> IoResult<PatriciaTree<T>> {
      Ok(PatriciaTree {
        skip_prefix: try!(prepend_err("skip_prefix", Serializable::deserialize(iter.by_ref()))),
        skip_len: try!(prepend_err("skip_len", Serializable::deserialize(iter.by_ref()))),
        data: try!(prepend_err("data", Serializable::deserialize(iter.by_ref()))),
        child_l: match iter.next() {
                   Some(1) => Some(box try!(prepend_err("child_l", recurse(iter)))),
                   Some(0) => None,
                   _ => { return Err(standard_error(InvalidInput)) }
                 },
        child_r: match iter.next() {
                   Some(1) => Some(box try!(prepend_err("child_r", recurse(iter)))),
                   Some(0) => None,
                   _ => { return Err(standard_error(InvalidInput)) }
                 }
      })
    }
    recurse(&mut iter.by_ref())
  }
}

#[cfg(test)]
mod tests {
  use std::prelude::*;
  use std::io::IoResult;
  use std::num::Zero;

  use util::hash::Sha256dHash;
  use util::uint256::Uint256;
  use util::patricia_tree::PatriciaTree;
  use network::serialize::Serializable;

  #[test]
  fn patricia_single_insert_lookup_delete_test() {
    let mut key = Uint256::from_u64(0xDEADBEEFDEADBEEF);
    key = key.shl(64).add(&key);

    let mut tree = PatriciaTree::new();
    tree.insert(&key, 100, 100u32);
    tree.insert(&key, 120, 100u32);

    assert_eq!(tree.lookup(&key, 100), Some(&100u32));
    assert_eq!(tree.lookup(&key, 101), None);
    assert_eq!(tree.lookup(&key, 99), None);
    assert_eq!(tree.delete(&key, 100), Some(100u32));
  }

  #[test]
  fn patricia_insert_lookup_delete_test() {
    let mut tree = PatriciaTree::new();
    let mut hashes = vec![];
    for i in range(0u32, 5000) {
      let hash = Sha256dHash::from_data(&[(i / 0x100) as u8, (i % 0x100) as u8]).as_uint256();
      tree.insert(&hash, 250, i);
      hashes.push(hash);
    }

    // Check that all inserts are correct
    for (n, hash) in hashes.iter().enumerate() {
      let ii = n as u32;
      let ret = tree.lookup(hash, 250);
      assert_eq!(ret, Some(&ii));
    }

    // Delete all the odd-numbered nodes
    for (n, hash) in hashes.iter().enumerate() {
      if n % 2 == 1 {
        let ii = n as u32;
        let ret = tree.delete(hash, 250);
        assert_eq!(ret, Some(ii));
      }
    }

    // Confirm all is correct
    for (n, hash) in hashes.iter().enumerate() {
      let ii = n as u32;
      let ret = tree.lookup(hash, 250);
      if n % 2 == 0 {
        assert_eq!(ret, Some(&ii));
      } else {
        assert_eq!(ret, None);
      }
    }
  }

  #[test]
  fn patricia_insert_substring_keys() {
    // This test uses a bunch of keys that are substrings of each other
    // to make sure insertion and deletion does not lose data
    let mut tree = PatriciaTree::new();
    let mut hashes = vec![];
    // Start by inserting a bunch of chunder
    for i in range(1u32, 500) {
      let hash = Sha256dHash::from_data(&[(i / 0x100) as u8, (i % 0x100) as u8]).as_uint256();
      tree.insert(&hash, 256, i * 1000);
      hashes.push(hash);
    }
    // Do the actual test -- note that we also test insertion and deletion
    // at the root here.
    for i in range(0u32, 10) {
      tree.insert(&Zero::zero(), i as uint, i);
    }
    for i in range(0u32, 10) {
      let m = tree.lookup(&Zero::zero(), i as uint);
      assert_eq!(m, Some(&i));
    }
    for i in range(0u32, 10) {
      let m = tree.delete(&Zero::zero(), i as uint);
      assert_eq!(m, Some(i));
    }
    // Check that the chunder was unharmed
    for (n, hash) in hashes.iter().enumerate() {
      let ii = ((n + 1) * 1000) as u32;
      let ret = tree.lookup(hash, 256);
      assert_eq!(ret, Some(&ii));
    }
  }

  #[test]
  fn patricia_serialize_test() {
    // Build a tree
    let mut tree = PatriciaTree::new();
    let mut hashes = vec![];
    for i in range(0u32, 5000) {
      let hash = Sha256dHash::from_data(&[(i / 0x100) as u8, (i % 0x100) as u8]).as_uint256();
      tree.insert(&hash, 250, i);
      hashes.push(hash);
    }

    // Serialize it
    let serialized = tree.serialize();
    // Check iterator
    let serialized_1 = tree.serialize_iter().collect();
    assert_eq!(serialized, serialized_1);
    // Deserialize it
    let deserialized: IoResult<PatriciaTree<u32>> = Serializable::deserialize(serialized.iter().map(|n| *n));
    assert!(deserialized.is_ok());
    let new_tree = deserialized.unwrap();

    // Check that all inserts are still there
    for (n, hash) in hashes.iter().enumerate() {
      let ii = n as u32;
      let ret = new_tree.lookup(hash, 250);
      assert_eq!(ret, Some(&ii));
    }
  }
}

