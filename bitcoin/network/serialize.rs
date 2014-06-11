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

use std::io::{IoError, IoResult, InvalidInput, OtherIoError, standard_error};
use std::mem::{to_le16, to_le32, to_le64};
use std::mem::transmute;

use util::iter::{FixedTake, FixedTakeable};
use util::hash::Sha256dHash;

#[deriving(PartialEq, Clone, Show)]
pub struct CommandString {
  data: String
}

impl CommandString {
  pub fn new(data: &str) -> CommandString {
    CommandString {
      data: String::from_str(data)
    }
  }
}

impl Str for CommandString {
  fn as_slice<'a>(&'a self) -> &'a str {
    self.data.as_slice()
  }
}

#[deriving(PartialEq, Clone, Show)]
pub struct CheckedData {
  data: Vec<u8>
}

impl CheckedData {
  pub fn from_vec(data: Vec<u8>) -> CheckedData {
    CheckedData { data: data }
  }

  pub fn data(self) -> Vec<u8> {
    self.data
  }
}

/// A message which can be sent on the Bitcoin network
pub trait Serializable : Send {
  /// Turn an object into a bytestring that can be put on the wire
  fn serialize(&self) -> Vec<u8>;
  /// Read an object off the wire
  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<Self>;
}

pub trait Message : Serializable {
  fn command(&self) -> CommandString;
}

pub enum VarInt {
  VarU8(u8),
  VarU16(u16),
  VarU32(u32),
  VarU64(u64)
}

/// Utility functions
pub fn u64_to_varint(n: u64) -> VarInt {
  match n {
    n if n < 0xFD => VarU8(n as u8),
    n if n <= 0xFFFF => VarU16(n as u16),
    n if n <= 0xFFFFFFFF => VarU32(n as u32),
    n => VarU64(n)
  }
}

pub fn varint_to_u64(n: VarInt) -> u64 {
  match n {
    VarU8(m) => m as u64,
    VarU16(m) => m as u64,
    VarU32(m) => m as u64,
    VarU64(m) => m,
  }
}

fn read_uint_le<I: Iterator<u8>>(mut iter: FixedTake<I>) -> Option<u64> {
  let (rv, _) = iter.fold((0u64, 1u64), |(old, mult), next| (old + next as u64 * mult, mult * 0x100));
  match iter.is_err() {
    false => Some(rv),
    true => None
  }
}

/// Do a double-SHA256 on some data and return the first 4 bytes
fn sha2_checksum(data: &[u8]) -> u32 {
  let checksum = Sha256dHash::from_data(data);
  read_uint_le(checksum.data().iter().map(|n| *n).fixed_take(4)).unwrap() as u32
}

/// Primitives
impl Serializable for bool {
  fn serialize(&self) -> Vec<u8> {
    if *self { Vec::from_slice(&[1u8]) } else { Vec::from_slice(&[0u8]) }
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<bool> {
    match iter.next() {
      Some(u) => Ok(u != 0),
      None    => Err(standard_error(InvalidInput))
    }
  }
}

impl Serializable for u8 {
  fn serialize(&self) -> Vec<u8> {
    Vec::from_slice(&[*self])
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<u8> {
    match iter.next() {
      Some(u) => Ok(u as u8),
      None    => Err(standard_error(InvalidInput))
    }
  }
}

impl Serializable for u16 {
  fn serialize(&self) -> Vec<u8> {
    unsafe { Vec::from_slice(transmute::<_, [u8, ..2]>(to_le16(*self))) }
  }

  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<u16> {
    match read_uint_le(iter.fixed_take(2)) {
      Some(u) => Ok(u as u16),
      None    => Err(standard_error(InvalidInput))
    }
  }
}

impl Serializable for u32 {
  fn serialize(&self) -> Vec<u8> {
    unsafe { Vec::from_slice(transmute::<_, [u8, ..4]>(to_le32(*self))) }
  }

  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<u32> {
    match read_uint_le(iter.fixed_take(4)) {
      Some(u) => Ok(u as u32),
      None    => Err(standard_error(InvalidInput))
    }
  }
}

impl Serializable for i32 {
  fn serialize(&self) -> Vec<u8> {
    unsafe { Vec::from_slice(transmute::<_, [u8, ..4]>(to_le32(*self as u32))) }
  }

  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<i32> {
    match read_uint_le(iter.fixed_take(4)) {
      Some(u) => Ok(u as i32),
      None    => Err(standard_error(InvalidInput))
    }
  }
}

impl Serializable for u64 {
  fn serialize(&self) -> Vec<u8> {
    unsafe { Vec::from_slice(transmute::<_, [u8, ..8]>(to_le64(*self))) }
  }

  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<u64> {
    match read_uint_le(iter.fixed_take(8)) {
      Some(u) => Ok(u as u64),
      None    => Err(standard_error(InvalidInput))
    }
  }
}

impl Serializable for i64 {
  fn serialize(&self) -> Vec<u8> {
    unsafe { Vec::from_slice(transmute::<_, [u8, ..8]>(to_le64(*self as u64))) }
  }

  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<i64> {
    match read_uint_le(iter.fixed_take(8)) {
      Some(u) => Ok(u as i64),
      None    => Err(standard_error(InvalidInput))
    }
  }
}

impl Serializable for VarInt {
  fn serialize(&self) -> Vec<u8> {
    match *self {
      VarU8(n)  => Vec::from_slice(&[n]),
      VarU16(n) => { let mut rv = n.serialize(); rv.unshift(0xFD); rv },
      VarU32(n) => { let mut rv = n.serialize(); rv.unshift(0xFE); rv },
      VarU64(n) => { let mut rv = n.serialize(); rv.unshift(0xFF); rv },
    }
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<VarInt> {
    match iter.next() {
      Some(n) if n < 0xFD => Ok(VarU8(n)),
      Some(n) if n == 0xFD => Ok(VarU16(try!(Serializable::deserialize(iter)))),
      Some(n) if n == 0xFE => Ok(VarU32(try!(Serializable::deserialize(iter)))),
      Some(n) if n == 0xFF => Ok(VarU64(try!(Serializable::deserialize(iter)))),
      _ => Err(standard_error(InvalidInput))
    }
  }
}

macro_rules! serialize_fixvec(
  ($($size:expr),+) => (
    $(
      impl Serializable for [u8, ..$size] {
        fn serialize(&self) -> Vec<u8> {
          Vec::from_slice(self.as_slice())
        }

        fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<[u8, ..$size]> {
          let mut v = [0u8, ..$size];
          let mut fixiter = iter.fixed_take($size);
          let mut n = 0;
          for ch in fixiter {
            v[n] = ch;
            n += 1;
          }
          match fixiter.is_err() {
            false => Ok(v),
            true => Err(standard_error(InvalidInput))
          }
        }
      }
    )+

    #[test]
    fn test_fixvec() {
      $(
        let vec = [5u8, ..$size];
        let short_vec = [5u8, ..($size - 1)];
        assert_eq!(vec.as_slice(), vec.serialize().as_slice());

        let decode: IoResult<[u8, ..$size]> = Serializable::deserialize(vec.iter().map(|n| *n));
        let short_decode: IoResult<[u8, ..$size]> = Serializable::deserialize(short_vec.iter().map(|n| *n));

        assert!(decode.is_ok());
        assert!(short_decode.is_err());
        assert_eq!(decode.unwrap().as_slice(), vec.as_slice());
      )+
    }
  );
)
// we need to do this in one call so that we can do a test for
// every value; we can't define a new test fn for each invocation
// because there are no gensyms.
serialize_fixvec!(4, 12, 16, 32)

impl Serializable for CheckedData {
  fn serialize(&self) -> Vec<u8> {
    let mut ret = (self.data.len() as u32).serialize();
    ret.extend(sha2_checksum(self.data.as_slice()).serialize().move_iter());
    ret.extend(self.data.iter().map(|n| *n));
    ret
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<CheckedData> {
    let length: u32 = try!(Serializable::deserialize(iter.by_ref()));
    let checksum: u32 = try!(Serializable::deserialize(iter.by_ref()));

    let mut fixiter = iter.fixed_take(length as uint);
    let v: Vec<u8> =  FromIterator::from_iter(fixiter.by_ref());
    if fixiter.is_err() {
      return Err(standard_error(InvalidInput));
    }

    let expected_checksum = sha2_checksum(v.as_slice());
    if checksum == expected_checksum {
      Ok(CheckedData::from_vec(v))
    } else {
      Err(IoError {
        kind: OtherIoError,
        desc: "bad checksum",
        detail: Some(format!("checksum {:4x} did not match expected {:4x}", checksum, expected_checksum)),
      })
    }
  }
}

impl Serializable for String {
  fn serialize(&self) -> Vec<u8> {
    let mut rv = u64_to_varint(self.len() as u64).serialize();
    rv.push_all(self.as_bytes());
    rv
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<String> {
    let length: VarInt = try!(Serializable::deserialize(iter.by_ref()));
    let mut fixiter = iter.fixed_take(varint_to_u64(length) as uint);
    let rv: String = FromIterator::from_iter(fixiter.by_ref().map(|u| u as char));
    match fixiter.is_err() {
      false => Ok(rv),
      true => Err(standard_error(InvalidInput))
    }
  }
}

impl Serializable for CommandString {
  fn serialize(&self) -> Vec<u8> {
    let mut rawbytes = [0u8, ..12]; 
    rawbytes.copy_from(self.data.as_bytes().as_slice());
    Vec::from_slice(rawbytes.as_slice())
  }

  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<CommandString> {
    let mut fixiter = iter.fixed_take(12);
    let rv: String = FromIterator::from_iter(fixiter.by_ref().filter_map(|u| if u > 0 { Some(u as char) } else { None }));
    // Once we've read the string, run out the iterator
    for _ in fixiter {}
    match fixiter.is_err() {
      false => Ok(CommandString { data: rv }),
      true => Err(standard_error(InvalidInput))
    }
  }
}

impl<T: Serializable> Serializable for Vec<T> {
  fn serialize(&self) -> Vec<u8> {
    let n_elems = match self.len() { 
      n if n > 0xFFFFFFFF => VarU64(n as u64),
      n if n > 0xFFFF     => VarU32(n as u32),
      n if n > 0xFC       => VarU16(n as u16),
      n => VarU8(n as u8)
    };
    let mut rv = n_elems.serialize();
    for elem in self.iter() {
      rv.extend(elem.serialize().move_iter());
    }
    rv
  }

  fn deserialize<I: Iterator<u8>>(mut iter: I) -> IoResult<Vec<T>> {
    let mut n_elems = varint_to_u64(try!(Serializable::deserialize(iter.by_ref())));
    let mut v: Vec<T> = vec![];
    while n_elems > 0 {
      v.push(try!(Serializable::deserialize(iter.by_ref())));
      n_elems -= 1;
    }
    Ok(v)
  }
}

#[test]
fn serialize_int_test() {
  // bool
  assert_eq!(false.serialize(), Vec::from_slice([0u8]));
  assert_eq!(true.serialize(), Vec::from_slice([1u8]));
  // u8
  assert_eq!(1u8.serialize(), Vec::from_slice([1u8]));
  assert_eq!(0u8.serialize(), Vec::from_slice([0u8]));
  assert_eq!(255u8.serialize(), Vec::from_slice([255u8]));
  // u16
  assert_eq!(1u16.serialize(), Vec::from_slice([1u8, 0]));
  assert_eq!(256u16.serialize(), Vec::from_slice([0u8, 1]));
  assert_eq!(5000u16.serialize(), Vec::from_slice([136u8, 19]));
  // u32
  assert_eq!(1u32.serialize(), Vec::from_slice([1u8, 0, 0, 0]));
  assert_eq!(256u32.serialize(), Vec::from_slice([0u8, 1, 0, 0]));
  assert_eq!(5000u32.serialize(), Vec::from_slice([136u8, 19, 0, 0]));
  assert_eq!(500000u32.serialize(), Vec::from_slice([32u8, 161, 7, 0]));
  assert_eq!(168430090u32.serialize(), Vec::from_slice([10u8, 10, 10, 10]));
  // TODO: test negative numbers
  assert_eq!(1i32.serialize(), Vec::from_slice([1u8, 0, 0, 0]));
  assert_eq!(256i32.serialize(), Vec::from_slice([0u8, 1, 0, 0]));
  assert_eq!(5000i32.serialize(), Vec::from_slice([136u8, 19, 0, 0]));
  assert_eq!(500000i32.serialize(), Vec::from_slice([32u8, 161, 7, 0]));
  assert_eq!(168430090i32.serialize(), Vec::from_slice([10u8, 10, 10, 10]));
  // u64
  assert_eq!(1u64.serialize(), Vec::from_slice([1u8, 0, 0, 0, 0, 0, 0, 0]));
  assert_eq!(256u64.serialize(), Vec::from_slice([0u8, 1, 0, 0, 0, 0, 0, 0]));
  assert_eq!(5000u64.serialize(), Vec::from_slice([136u8, 19, 0, 0, 0, 0, 0, 0]));
  assert_eq!(500000u64.serialize(), Vec::from_slice([32u8, 161, 7, 0, 0, 0, 0, 0]));
  assert_eq!(723401728380766730u64.serialize(), Vec::from_slice([10u8, 10, 10, 10, 10, 10, 10, 10]));
  // TODO: test negative numbers
  assert_eq!(1i64.serialize(), Vec::from_slice([1u8, 0, 0, 0, 0, 0, 0, 0]));
  assert_eq!(256i64.serialize(), Vec::from_slice([0u8, 1, 0, 0, 0, 0, 0, 0]));
  assert_eq!(5000i64.serialize(), Vec::from_slice([136u8, 19, 0, 0, 0, 0, 0, 0]));
  assert_eq!(500000i64.serialize(), Vec::from_slice([32u8, 161, 7, 0, 0, 0, 0, 0]));
  assert_eq!(723401728380766730i64.serialize(), Vec::from_slice([10u8, 10, 10, 10, 10, 10, 10, 10]));
}

#[test]
fn serialize_varint_test() {
  assert_eq!(VarU8(10).serialize(), Vec::from_slice([10u8]));
  assert_eq!(VarU8(0xFC).serialize(), Vec::from_slice([0xFCu8]));
  assert_eq!(VarU16(0xFD).serialize(), Vec::from_slice([0xFDu8, 0xFD, 0]));
  assert_eq!(VarU16(0xFFF).serialize(), Vec::from_slice([0xFDu8, 0xFF, 0xF]));
  assert_eq!(VarU32(0xF0F0F0F).serialize(), Vec::from_slice([0xFEu8, 0xF, 0xF, 0xF, 0xF]));
  assert_eq!(VarU64(0xF0F0F0F0F0E0).serialize(), Vec::from_slice([0xFFu8, 0xE0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0, 0]));
}

#[test]
fn serialize_vector_test() {
  assert_eq!(Vec::from_slice([1u8, 2, 3]).serialize(), Vec::from_slice([3u8, 1, 2, 3]));
  // TODO: test vectors of more interesting objects
}

#[test]
fn serialize_strbuf_test() {
  assert_eq!(String::from_str("Andrew").serialize(), Vec::from_slice([6u8, 0x41, 0x6e, 0x64, 0x72, 0x65, 0x77]));
}

#[test]
fn serialize_commandstring_test() {
  let cs = CommandString::new("Andrew");
  assert_eq!(cs.as_slice(), "Andrew");
  assert_eq!(cs.serialize(), vec![0x41u8, 0x6e, 0x64, 0x72, 0x65, 0x77, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn serialize_checkeddata_test() {
  let cd = CheckedData::from_vec(vec![1u8, 2, 3, 4, 5]);
  assert_eq!(cd.serialize(), vec![5, 0, 0, 0, 162, 107, 175, 90, 1, 2, 3, 4, 5]);
}

#[test]
fn deserialize_int_test() {
  // bool
  assert_eq!(Serializable::deserialize([58u8, 0].iter().map(|n| *n)), Ok(true));
  assert_eq!(Serializable::deserialize([58u8].iter().map(|n| *n)), Ok(true));
  assert_eq!(Serializable::deserialize([1u8].iter().map(|n| *n)), Ok(true));
  assert_eq!(Serializable::deserialize([0u8].iter().map(|n| *n)), Ok(false));
  assert_eq!(Serializable::deserialize([0u8, 1].iter().map(|n| *n)), Ok(false));

  // u8
  assert_eq!(Serializable::deserialize([58u8].iter().map(|n| *n)), Ok(58u8));

  // u16
  assert_eq!(Serializable::deserialize([0x01u8, 0x02].iter().map(|n| *n)), Ok(0x0201u16));
  assert_eq!(Serializable::deserialize([0xABu8, 0xCD].iter().map(|n| *n)), Ok(0xCDABu16));
  assert_eq!(Serializable::deserialize([0xA0u8, 0x0D].iter().map(|n| *n)), Ok(0xDA0u16));
  let failure16: IoResult<u16> = Serializable::deserialize([1u8].iter().map(|n| *n));
  assert!(failure16.is_err());

  // u32
  assert_eq!(Serializable::deserialize([0xABu8, 0xCD, 0, 0].iter().map(|n| *n)), Ok(0xCDABu32));
  assert_eq!(Serializable::deserialize([0xA0u8, 0x0D, 0xAB, 0xCD].iter().map(|n| *n)), Ok(0xCDAB0DA0u32));
  let failure32: IoResult<u32> = Serializable::deserialize([1u8, 2, 3].iter().map(|n| *n));
  assert!(failure32.is_err());
  // TODO: test negative numbers
  assert_eq!(Serializable::deserialize([0xABu8, 0xCD, 0, 0].iter().map(|n| *n)), Ok(0xCDABi32));
  assert_eq!(Serializable::deserialize([0xA0u8, 0x0D, 0xAB, 0x2D].iter().map(|n| *n)), Ok(0x2DAB0DA0i32));
  let failurei32: IoResult<i32> = Serializable::deserialize([1u8, 2, 3].iter().map(|n| *n));
  assert!(failurei32.is_err());

  // u64
  assert_eq!(Serializable::deserialize([0xABu8, 0xCD, 0, 0, 0, 0, 0, 0].iter().map(|n| *n)), Ok(0xCDABu64));
  assert_eq!(Serializable::deserialize([0xA0u8, 0x0D, 0xAB, 0xCD, 0x99, 0, 0, 0x99].iter().map(|n| *n)), Ok(0x99000099CDAB0DA0u64));
  let failure64: IoResult<u64> = Serializable::deserialize([1u8, 2, 3, 4, 5, 6, 7].iter().map(|n| *n));
  assert!(failure64.is_err());
  // TODO: test negative numbers
  assert_eq!(Serializable::deserialize([0xABu8, 0xCD, 0, 0, 0, 0, 0, 0].iter().map(|n| *n)), Ok(0xCDABi64));
  assert_eq!(Serializable::deserialize([0xA0u8, 0x0D, 0xAB, 0xCD, 0x99, 0, 0, 0x99].iter().map(|n| *n)), Ok(0x99000099CDAB0DA0i64));
  let failurei64: IoResult<i64> = Serializable::deserialize([1u8, 2, 3, 4, 5, 6, 7].iter().map(|n| *n));
  assert!(failurei64.is_err());
}

#[test]
fn deserialize_vec_test() {
  assert_eq!(Serializable::deserialize([3u8, 2, 3, 4].iter().map(|n| *n)), Ok(vec![2u8, 3, 4]));
  assert_eq!(Serializable::deserialize([4u8, 2, 3, 4, 5, 6].iter().map(|n| *n)), Ok(vec![2u8, 3, 4, 5]));
}

#[test]
fn deserialize_strbuf_test() {
  assert_eq!(Serializable::deserialize([6u8, 0x41, 0x6e, 0x64, 0x72, 0x65, 0x77].iter().map(|n| *n)), Ok(String::from_str("Andrew")));
}

#[test]
fn deserialize_commandstring_test() {
  let cs: IoResult<CommandString> = Serializable::deserialize([0x41u8, 0x6e, 0x64, 0x72, 0x65, 0x77, 0, 0, 0, 0, 0, 0].iter().map(|n| *n));
  assert!(cs.is_ok());
  assert_eq!(cs.unwrap(), CommandString { data: String::from_str("Andrew") });

  let short_cs: IoResult<CommandString> = Serializable::deserialize([0x41u8, 0x6e, 0x64, 0x72, 0x65, 0x77, 0, 0, 0, 0, 0].iter().map(|n| *n));
  assert!(short_cs.is_err());
}

#[test]
fn deserialize_checkeddata_test() {
  let cd: IoResult<CheckedData> = Serializable::deserialize([5u8, 0, 0, 0, 162, 107, 175, 90, 1, 2, 3, 4, 5].iter().map(|n| *n));
  assert!(cd.is_ok());
  assert_eq!(cd.unwrap().data().as_slice(), &[1u8, 2, 3, 4, 5]);
}








