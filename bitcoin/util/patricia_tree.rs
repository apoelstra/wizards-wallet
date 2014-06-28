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
use collections::bitv::Bitv;
use std::io::{IoResult, InvalidInput, standard_error};

use network::serialize::{Serializable, SerializeIter};
use util::misc::prepend_err;

/// Patricia troo
pub struct PatriciaTree<T> {
  data: Option<T>,
  child_l: Option<Box<PatriciaTree<T>>>,
  child_r: Option<Box<PatriciaTree<T>>>,
  skip_prefix: Bitv
}

impl<T> PatriciaTree<T> {
  /// Constructs a new Patricia tree
  pub fn new() -> PatriciaTree<T> {
    PatriciaTree {
      data: None,
      child_l: None,
      child_r: None,
      skip_prefix: Bitv::new(0, false),
    }
  }

  /// Lookup a value by exactly matching `key`
  pub fn lookup<'a>(&'a self, key: &Bitv) -> Option<&'a T> {
    fn recurse<'a, T, I: Iterator<bool>>(tree: &'a PatriciaTree<T>, mut search_key_iter: I) -> Option<&'a T> {
      let mut node_key_iter = tree.skip_prefix.iter();
      loop {
        match (search_key_iter.next(), node_key_iter.next()) {
          // Node key is exact match to search key
          (None, None) => {
            return match tree.data {
              Some(ref bx) => {
                let borrow = &*bx;
                Some(borrow)
              },
              _ => None
            };
          }
          // Node key runs out (node key is prefix of search key)
          (Some(s), None) => {
            let subtree = if s { &tree.child_r } else { &tree.child_l };
            return match subtree {
              &Some(ref bx) => {
                let borrow = &**bx;  // bx is a &Box<U> here, so &**bx gets &U
                recurse(borrow, search_key_iter)
              }
              &None => None
            }
          }
          // Search key runs out before node key (node key is a superstring of
          // search key) --- no match
          (None, _) => { return None }
          // Key fails to match prefix --- no match
          (Some(s), Some(n)) if s != n => { return None }
          // Key matches prefix (keep checking)
          (Some(_), Some(_)) => { }
        }
      }
    }
    recurse(self, key.iter())
  }

  /// Inserts a value with key `key`, returning true on success. If a value is already
  /// stored against `key`, do nothing and return false.
  pub fn insert(&mut self, key: &Bitv, value: T) -> bool {
    fn recurse<T, I:Iterator<bool>>(tree: &mut PatriciaTree<T>, mut search_key_iter: I, value: T) -> bool {
      // TODO: this clone() is totally unnecessary, requires non-lexically scoped borrows
      // to remove since we later overwrite tree.skip_prefix, invalidading the iterator,
      // and we can't signal the compiler that we won't use it again.
      let sp_clone = tree.skip_prefix.clone();
      let mut node_key_iter = sp_clone.iter();
      let mut prefix_key = Bitv::new(0, false);
      loop {
        match (search_key_iter.next(), node_key_iter.next()) {
          // Node key is exact match to search key --- key already used
          (None, None) => {
            if tree.data.is_none() {
              tree.data = Some(value);
              return true;
            }
            return false;
          }
          // Node key runs out (node key is prefix of search key)
          (Some(s), None) => {
            let subtree = if s { &mut tree.child_r } else { &mut tree.child_l };
            return match subtree {
              // Recurse if we can
              &Some(ref mut bx) => {
                let borrow = &mut **bx;
                recurse(borrow, search_key_iter, value)
              }
              // Otherwise insert the node here
              &None => {
                *subtree = Some(box PatriciaTree {
                  data: Some(value),
                  child_l: None,
                  child_r: None,
                  skip_prefix: search_key_iter.collect()
                });
                true
              }
            }
          }
          // Search key runs out before node key (node key is a superstring of
          // search key) --- we have to split the node key to insert the new
          // element
          (None, Some(kmid)) => {
            let ksuf_neighbor: Bitv = node_key_iter.collect();

            // Remove the old node's children
            let child_l = tree.child_l.take();
            let child_r = tree.child_r.take();
            let value_neighbor = tree.data.take();
            // Chop the prefix down and put the new data in place
            tree.skip_prefix = prefix_key;
            tree.data = Some(value);
            // Put the old data in a new child, with the remainder of the prefix
            let new_child = if kmid { &mut tree.child_r } else { &mut tree.child_l };
            *new_child = Some(box PatriciaTree {
              data: value_neighbor,
              child_l: child_l,
              child_r: child_r,
              skip_prefix: ksuf_neighbor
            });
            return true;
          }
          // Key fails to match prefix --- split the node key, move its data to
          // one child, and insert at the other child
          (Some(s), Some(n)) if s != n => {
            let ksuf_insert: Bitv = search_key_iter.collect();
            let ksuf_neighbor: Bitv = node_key_iter.collect();

            // Remove the old node's children
            let child_l = tree.child_l.take();
            let child_r = tree.child_r.take();
            let value_neighbor = tree.data.take();
            // Chop the prefix down
            tree.skip_prefix = prefix_key;
            let (insert, neighbor) = if s { (&mut tree.child_r, &mut tree.child_l) }
                                     else { (&mut tree.child_l, &mut tree.child_r) };
            *insert = Some(box PatriciaTree {
              data: Some(value),
              child_l: None,
              child_r: None,
              skip_prefix: ksuf_insert
            });
            *neighbor = Some(box PatriciaTree {
              data: value_neighbor,
              child_l: child_l,
              child_r: child_r,
              skip_prefix: ksuf_neighbor
            });
            return true;
          }
          // Key matches prefix (keep checking)
          (Some(_), Some(n)) => { prefix_key.push(n); }
        }
      }
    }
    recurse(self, key.iter(), value)
  }

  /// Deletes a value with key `key`, returning it on success. If no value with
  /// the given key is found, return None
  pub fn delete(&mut self, key: &Bitv) -> Option<T> {
    /// Return value is (just_returned, actual return value), where just_returned
    /// is true when returning from the iteration when the node was deleted, since
    /// that node's caller might need to rearrange its parents
    fn recurse<T, I:Iterator<bool>>(tree: &mut PatriciaTree<T>, mut search_key_iter: I) -> (bool, Option<T>) {
      // This clone is also unnecessary, because we later append to skip_prefix but
      // can't tell the compiler that we don't need the iterator anymore.
      let sp_clone = tree.skip_prefix.clone();
      let mut node_key_iter = sp_clone.iter();
      loop {
        match (search_key_iter.next(), node_key_iter.next()) {
          // Node key is exact match to search key
          (None, None) => {
            let ret = tree.data.take();
            return match (tree.child_l.take(), tree.child_r.take()) {
              // If we have no children, return true to signal caller to
              // delete the node
              (None, None) => (true, ret),
              // If we have two children, the tree structure does not
              // need to change.
              (Some(lc), Some(rc)) => {
                tree.child_l = Some(lc);
                tree.child_r = Some(rc);
                (false, ret)
              },
              // If we have one child, merge it down
              (None, Some(ref mut s)) => {
                tree.skip_prefix.push(true);
                for bit in s.skip_prefix.iter() {
                  tree.skip_prefix.push(bit);
                }
                tree.child_l = s.child_l.take();
                tree.child_r = s.child_r.take();
                tree.data = s.data.take();
                // After merging, don't tell caller to delete this node
                (false, ret)
              },
              (Some(ref mut s), None) => {
                tree.skip_prefix.push(false);
                for bit in s.skip_prefix.iter() {
                  tree.skip_prefix.push(bit);
                }
                tree.child_l = s.child_l.take();
                tree.child_r = s.child_r.take();
                // After merging, don't tell caller to delete this node
                (false, ret)
              }
            };
          }
          // Node key runs out (node key is prefix of search key)
          (Some(s), None) => {
            let mut to_remove = false;
            let ret = match if s { &mut tree.child_r } else { &mut tree.child_l } {
              &Some(ref mut bx) => {
                let borrow = &mut **bx;  // bx is a &Box<U> here, so &**bx gets &U
                let (just_removed, ret) = recurse(borrow, search_key_iter);
                to_remove = just_removed;
                ret
              }
              &None => None
            };
            if to_remove {
              // If the deleted node had a sibling, and this node has no data,
              // merge it down
              if tree.data.is_none() {
                let mut sibling = if s { tree.child_l.take() }
                                  else { tree.child_r.take() };
                match sibling {
                  Some(ref mut sib) => {
                    tree.child_l = sib.child_l.take();
                    tree.child_r = sib.child_r.take();
                    tree.skip_prefix.push(!s);
                    for bit in sib.skip_prefix.iter() {
                      tree.skip_prefix.push(bit);
                    }
                    tree.data = sib.data.take();
                  }
                  _ => {
                    tree.child_l = None;
                    tree.child_r = None;
                  }
                }
              } else {
                // Otherwise just delete the child and leave the rest alone
                if s { tree.child_r = None; }
                else { tree.child_l = None; }
              }
            }
            return (false, ret);
          }
          // Search key runs out before node key (node key is a superstring of
          // search key) --- no match
          (None, _) => { return (false, None); }
          // Key fails to match prefix --- no match
          (Some(s), Some(n)) if s != n => { return (false, None); }
          // Key matches prefix (keep checking)
          (Some(_), Some(_)) => { }
        }
      }
    }
    let (_, ret) = recurse(self, key.iter());
    ret
  }

}

impl<T:Show> PatriciaTree<T> {
  /// Print the entire tree
  pub fn print<'a>(&'a self) {
    fn recurse<'a, T:Show>(tree: &'a PatriciaTree<T>, depth: uint) {
      println!("{:}: {:}", tree.skip_prefix, tree.data);
      // left gets no indentation
      match tree.child_l {
        Some(ref t) => {
          for _ in range(0, depth + tree.skip_prefix.len()) {
            print!("-");
          }
          print!("0");
          recurse(&**t, depth + tree.skip_prefix.len() + 1);
        }
        None => { }
      }
      // right one gets indentation
      match tree.child_r {
        Some(ref t) => {
          for _ in range(0, depth + tree.skip_prefix.len()) {
            print!("_");
          }
          print!("1");
          recurse(&**t, depth + tree.skip_prefix.len() + 1);
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
    ret.extend(self.data.serialize().move_iter());
    ret.extend(self.child_l.serialize().move_iter());
    ret.extend(self.child_r.serialize().move_iter());
    ret
  }

  fn serialize_iter<'a>(&'a self) -> SerializeIter<'a> {
    SerializeIter {
      data_iter: None,
      sub_iter_iter: box vec![ &self.skip_prefix as &Serializable,
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
  use collections::bitv::Bitv;
  use std::io::IoResult;

  use util::hash::Sha256dHash;
  use util::patricia_tree::PatriciaTree;
  use network::serialize::Serializable;

  #[test]
  fn patricia_insert_lookup_delete_test() {
    let mut tree = PatriciaTree::new();
    let mut hashes = vec![];
    for i in range(0u32, 5000) {
      let hash = Sha256dHash::from_data(&[(i / 0x100) as u8, (i % 0x100) as u8]).as_bitv();
      tree.insert(&hash, i);
      hashes.push(hash);
    }

    // Check that all inserts are correct
    for (n, hash) in hashes.iter().enumerate() {
      let ii = n as u32;
      let ret = tree.lookup(hash);
      assert_eq!(ret, Some(&ii));
    }

    // Delete all the odd-numbered nodes
    for (n, hash) in hashes.iter().enumerate() {
      if n % 2 == 1 {
        let ii = n as u32;
        let ret = tree.delete(hash);
        assert_eq!(ret, Some(ii));
      }
    }

    // Confirm all is correct
    for (n, hash) in hashes.iter().enumerate() {
      let ii = n as u32;
      let ret = tree.lookup(hash);
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
      let hash = Sha256dHash::from_data(&[(i / 0x100) as u8, (i % 0x100) as u8]).as_bitv();
      tree.insert(&hash, i * 1000);
      hashes.push(hash);
    }
    // Do the actual test -- note that we also test insertion and deletion
    // at the root here.
    for i in range(0u32, 10) {
      tree.insert(&Bitv::new(i as uint, true), i);
    }
    for i in range(0u32, 10) {
      let m = tree.lookup(&Bitv::new(i as uint, true));
      assert_eq!(m, Some(&i));
    }
    for i in range(0u32, 10) {
      let m = tree.delete(&Bitv::new(i as uint, true));
      assert_eq!(m, Some(i));
    }
    // Check that the chunder was unharmed
    for (n, hash) in hashes.iter().enumerate() {
      let ii = ((n + 1) * 1000) as u32;
      let ret = tree.lookup(hash);
      assert_eq!(ret, Some(&ii));
    }
  }

  #[test]
  fn patricia_serialize_test() {
    // Build a tree
    let mut tree = PatriciaTree::new();
    let mut hashes = vec![];
    for i in range(0u32, 2) {
      let hash = Sha256dHash::from_data(&[(i / 0x100) as u8, (i % 0x100) as u8]).as_bitv();
      tree.insert(&hash, i);
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
      let ret = new_tree.lookup(hash);
      assert_eq!(ret, Some(&ii));
    }
  }
}

