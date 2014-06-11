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

use network::serialize::Serializable;
use blockdata::opcodes;
#[cfg(test)]
use util::misc::hex_bytes;

pub struct Script {
  data: Vec<u8>
}

impl Script {
  pub fn new() -> Script { Script { data: vec![] } }

  pub fn push_int(&mut self, data: int) {
    // We can special-case -1, 1-16
    if data == -1 || (data >= 1 && data <=16) {
      self.data.push(data as u8 + opcodes::TRUE);
      return;
    }
    // We can also special-case zero
    if data == 0 {
      self.data.push(opcodes::FALSE);
      return;
    }
    // Otherwise encode it as data
    self.push_scriptint(data);
  }

  // Push the int as data: little-endian signed-mangnitude representation
  pub fn push_scriptint(&mut self, data: int) {
    let neg = data < 0;

    let mut abs = if neg { -data } else { data } as uint;
    let mut v = vec![];
    while abs > 0xFF {
      v.push((abs & 0xFF) as u8);
      abs >>= 8;
    }
    // If the number's value causes the sign bit to be set, we need an extra
    // byte to get the correct value and correct sign bit
    if abs & 0x80 != 0 {
      v.push(abs as u8);
      v.push(if neg { 0x80u8 } else { 0u8 });
    }
    // Otherwise we just set the sign bit ourselves
    else {
      abs |= if neg { 0x80 } else { 0 };
      v.push(abs as u8);
    }
    // Finally we put the encoded int onto the stack
    self.push_slice(v.as_slice());
  }

  pub fn push_slice(&mut self, data: &[u8]) {
    // Start with a PUSH opcode
    match data.len() {
      n if n < opcodes::PUSHDATA1 as uint => { self.data.push(n as u8); },
      n if n < 0x100 => {
        self.data.push(opcodes::PUSHDATA1);
        self.data.push(n as u8);
      },
      n if n < 0x10000 => {
        self.data.push(opcodes::PUSHDATA2);
        self.data.push((n % 0x100) as u8);
        self.data.push((n / 0x100) as u8);
      },
      n if n < 0x100000000 => {
        self.data.push(opcodes::PUSHDATA4);
        self.data.push((n % 0x100) as u8);
        self.data.push(((n / 0x100) % 0x100) as u8);
        self.data.push(((n / 0x10000) % 0x100) as u8);
        self.data.push((n / 0x1000000) as u8);
      }
      _ => fail!("tried to put a 4bn+ sized object into a script!")
    }
    // Then push the actual data
    self.data.extend(data.iter().map(|n| *n));
  }

  pub fn push_opcode(&mut self, data: u8) {
    self.data.push(data);
  }
}

impl Serializable for Script {
  fn serialize(&self) -> Vec<u8> { self.data.serialize() }
  fn deserialize<I: Iterator<u8>>(iter: I) -> IoResult<Script> {
    Ok(Script { data: try!(Serializable::deserialize(iter)) })
  }
}

#[test]
fn test_script() {
  let mut comp = vec![];
  let mut script = Script::new();
  assert_eq!(script.data, vec![]);

  // small ints
  script.push_int(1);  comp.push(82u8); assert_eq!(script.data, comp);
  script.push_int(0);  comp.push(0u8);  assert_eq!(script.data, comp);
  script.push_int(4);  comp.push(85u8); assert_eq!(script.data, comp);
  script.push_int(-1); comp.push(80u8); assert_eq!(script.data, comp);
  // forced scriptint
  script.push_scriptint(4);  comp.push_all([1u8, 4]); assert_eq!(script.data, comp);
  // big ints
  script.push_int(17); comp.push_all([1u8, 17]); assert_eq!(script.data, comp);
  script.push_int(10000); comp.push_all([2u8, 16, 39]); assert_eq!(script.data, comp);
  // notice the sign bit set here, hence the extra zero/128 at the end
  script.push_int(10000000); comp.push_all([4u8, 128, 150, 152, 0]); assert_eq!(script.data, comp);
  script.push_int(-10000000); comp.push_all([4u8, 128, 150, 152, 128]); assert_eq!(script.data, comp);

  // data
  script.push_slice("NRA4VR".as_bytes()); comp.push_all([6u8, 78, 82, 65, 52, 86, 82]); assert_eq!(script.data, comp);

  // opcodes 
  script.push_opcode(opcodes::CHECKSIG); comp.push(0xACu8); assert_eq!(script.data, comp);
  script.push_opcode(opcodes::CHECKSIG); comp.push(0xACu8); assert_eq!(script.data, comp);
}

#[test]
fn test_script_serialize() {
  let hex_script = hex_bytes("6c493046022100f93bb0e7d8db7bd46e40132d1f8242026e045f03a0efe71bbb8e3f475e970d790221009337cd7f1f929f00cc6ff01f03729b069a7c21b59b1736ddfee5db5946c5da8c0121033b9b137ee87d5a812d6f506efdd37f0affa7ffc310711c06c7f3e097c9447c52").unwrap();
  let script: IoResult<Script> = Serializable::deserialize(hex_script.iter().map(|n| *n));
  assert!(script.is_ok());
  assert_eq!(script.unwrap().serialize().as_slice(), hex_script.as_slice());
}


