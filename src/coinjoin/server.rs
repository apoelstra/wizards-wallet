/* The Wizards' Wallet
 * Written in 2014 by
 *   Andrew Poelstra <apoelstra@wpsoftware.net>
 *
 * To the extent possible under law, the author(s) have dedicated all
 * copyright and related and neighboring rights to this software to
 * the public domain worldwide. This software is distributed without
 * any warranty.
 *
 * You should have received a copy of the CC0 Public Domain Dedication
 * along with this software.
 * If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
 */

//! # Coinjoin Server
//!
//! Functions and data to manage a centralized coinjoin server.

use std::collections::{HashMap, TreeMap};
use std::default::Default;
use std::num::from_str_radix;
use std::io::IoResult;
use std::rand::{Rng, SeedableRng};
use std::time::Duration;
use serialize::json;
use serialize::{Decodable, Decoder, Encodable, Encoder};
use time::precise_time_ns;

use bitcoin::blockdata::transaction::{Transaction, TxIn, PayToPubkeyHash};
use bitcoin::blockdata::utxoset::UtxoSet;
use bitcoin::network::serialize::{BitcoinHash, serialize_hex};
use bitcoin::util::base58::ToBase58;
use bitcoin::wallet::address::Address;

use crypto::fortuna::Fortuna;

use coinjoin::{CoinjoinError, DuplicateInput, IncorrectState, InsufficientFee,
               NoNewSignedInputs, NonZeroLocktime, NoTargetOutput,
               InputsExceedOutputs, OutputsExceedInputs, UnexpectedInput, UnexpectedOutput,
               UnknownInput, UnknownVersion, WrongInputCount, WrongOutputCount};

/// Current state of the session
#[deriving(Clone, PartialEq, Eq, PartialOrd, Ord, Show)]
pub enum SessionState {
  /// Collecting unsigned transactions
  Joining,
  /// Collecting signed transactions
  Merging,
  /// Completed successfully
  Complete,
  /// Timed out waiting on signed transactions
  Expired,
  /// Failed (input spent out from under it)
  Failed,
  /// Failed (not enough inputs)
  Unmerged
}

impl json::ToJson for SessionState {
  fn to_json(&self) -> json::Json {
    json::String(match *self {
      Joining => "joining",
      Merging => "merging",
      Complete => "complete",
      Expired => "expired",
      Failed => "failed",
      Unmerged => "unmerged"
    }.to_string())
  }
}

/// A Coinjoin session
pub struct Session {
  id: SessionId,
  rng: Fortuna,
  state: SessionState,
  // Time at which last state switch occured
  switch_time: u64,
  // Duration of "collecting unsigned transactions" phase
  join_duration: Duration,
  // Duration of every other phase before we expire or delete the session
  expiry_duration: Duration,
  target_value: u64,
  unsigned: Vec<Transaction>,
  merged: Option<Transaction>,
  signed: Option<Transaction>,
  donation_address: Address
}

impl json::ToJson for Session {
  fn to_json(&self) -> json::Json {
    let time_since_switch = Duration::nanoseconds(precise_time_ns() as i64 - self.switch_time as i64);

    let mut obj = TreeMap::new();
    obj.insert("id".to_string(), self.id.to_json());
    obj.insert("state".to_string(), self.state.to_json());
    obj.insert("join_duration".to_string(), self.join_duration.num_milliseconds().to_json());
    obj.insert("merge_duration".to_string(), self.expiry_duration.num_milliseconds().to_json());
    match self.state {
      Merging => {
        obj.insert("merged_tx".to_string(), json::String(serialize_hex(self.merged.as_ref().unwrap()).unwrap()));
        obj.insert("time_until_expiry".to_string(),
                   (self.expiry_duration - time_since_switch).num_milliseconds().to_json());
      }
      Joining => {
        obj.insert("time_until_merge".to_string(),
                   (self.join_duration - time_since_switch).num_milliseconds().to_json());
        obj.insert("donation_address".to_string(),
                   json::String(self.donation_address.to_base58check()));
      }
      Complete => {
        obj.insert("txid".to_string(), self.signed.as_ref().unwrap().bitcoin_hash().to_json());
        obj.insert("time_until_deletion".to_string(),
                   (self.expiry_duration - time_since_switch).num_milliseconds().to_json());
      }
      _ => {
        obj.insert("time_until_deletion".to_string(),
                   (self.expiry_duration - time_since_switch).num_milliseconds().to_json());
      }
    }
    obj.insert("target_value".to_string(), self.target_value.to_json());
    json::Object(obj)
  }
}

/// A session identifier
#[deriving(Hash, PartialEq, Eq, Clone, Show)]
pub struct SessionId(u64);

impl<E: Encoder<S>, S> Encodable<E, S> for SessionId {
  fn encode(&self, e: &mut E) -> Result<(), S> {
    let &SessionId(num) = self;
    e.emit_str(format!("{:08x}", num).as_slice())
  }
}

impl<D: Decoder<E>, E> Decodable<D, E> for SessionId {
  fn decode(d: &mut D) -> Result<SessionId, E> {
    let st = try!(d.read_str());
    match from_str_radix(st.as_slice(), 16) {
      Some(n) => Ok(SessionId(n)),
      None    => Err(d.error(format!("Session ID `{}` is not a valid hex string", st).as_slice()))
    }
  }
}

impl json::ToJson for SessionId {
  fn to_json(&self) -> json::Json {
    let &SessionId(num) = self;
    json::String(format!("{:08x}", num))
  }
}

impl Session {
  /// Creates a new session with a random ID
  pub fn new(target_value: u64,
             join_duration: Duration,
             expiry_duration: Duration,
             donation_address: Address)
             -> IoResult<Session> {
    use std::rand;
    let mut csrng: Fortuna = {
      let mut rng = try!(rand::OsRng::new());
      let mut seed = [0, ..256];
      rng.fill_bytes(seed.as_mut_slice());
      SeedableRng::from_seed(seed.as_slice())
    };
    let id = SessionId(csrng.gen());
    Ok(Session {
      id: id,
      rng: csrng,
      target_value: target_value,
      state: Joining,
      switch_time: precise_time_ns(),
      join_duration: join_duration,
      expiry_duration: expiry_duration,
      unsigned: vec![],
      merged: None,
      signed: None,
      donation_address: donation_address
    })
  }

  /// Retrieves the immutable ID of the session
  pub fn id(&self) -> SessionId {
    self.id
  }

  /// Adds an unsigned transaction to a coinjoin session
  pub fn add_unsigned(&mut self, tx: &Transaction, utxo_set: &UtxoSet)
                      -> Result<(), CoinjoinError> {
    if self.state != Joining {
      return Err(IncorrectState(Joining, self.state));
    }

    // Check for version, locktime
    if tx.version != 1 {
      return Err(UnknownVersion(tx.version as uint));
    }
    if tx.lock_time != 0 {
      return Err(NonZeroLocktime(tx.lock_time as uint));
    }

    // Check for output of the correct size
    if !tx.output.iter().any(|o| o.value == self.target_value) {
      return Err(NoTargetOutput(self.target_value));
    }

    // Check for fee
    let mut received_fee = 0;
    let required_fee = tx.input.len() as u64 * 200 + tx.output.len() as u64 * 50;
    for out in tx.output.iter() {
      match out.classify(self.donation_address.network) {
        PayToPubkeyHash(ref addr) => {
          if addr == &self.donation_address {
            received_fee += out.value;
          }
        }
        _ => {}
      }
    }
    if received_fee < required_fee {
      return Err(InsufficientFee(received_fee, required_fee));
    }

    // Check that we know all the inputs, and that they have
    // not already been used in this join
    let mut total_in = 0;
    for input in tx.input.iter() {
      let utxo = utxo_set.get_utxo(input.prev_hash, input.prev_index);
      match utxo {
        Some((_, out)) => { total_in += out.value; }
        None => { return Err(UnknownInput(input.prev_hash, input.prev_index as uint)); }
      }
      for other_tx in self.unsigned.iter() { 
        for other_input in other_tx.input.iter() {
          if input.prev_hash  == other_input.prev_hash &&
             input.prev_index == other_input.prev_index {
            return Err(DuplicateInput(input.prev_hash, input.prev_index as uint));
          }
        }
      }
    }
    let total_out = tx.output.iter().fold(0, |acc, out| acc + out.value);

    // Check that input value is <= output value
    // TODO: there should be a session option to allow this, for doing transfers
    if total_in < total_out {
      return Err(OutputsExceedInputs(total_out, total_in));
    }
    // TODO: there should be a session option to allow this and/or disabling donations
    if total_in > total_out {
      return Err(InputsExceedOutputs(total_in, total_out));
    }

    // All Ok, add it
    self.unsigned.push(tx.clone());
    Ok(())
  }

  // Merges all the transactions. Shouldn't be public, this should require
  // setting the status to `Merging`
  fn merge_transactions(&mut self) {
    let mut merged = Transaction {
      version: 1,
      lock_time: 0,
      input: Vec::with_capacity(
        self.unsigned.iter().fold(0, |acc, tx| acc + tx.input.len())),
      output: Vec::with_capacity(
        self.unsigned.iter().fold(0, |acc, tx| acc + tx.output.len())),
    };

    // We validated inputs and outputs when bringing them in
    for tx in self.unsigned.iter() {
      merged.input.push_all(tx.input.as_slice());

      // When adding outputs, check for dupes and consolitate them
      for out in tx.output.iter() {
        let mut already_exists = false;
        for existing_out in merged.output.mut_iter() {
          if existing_out.script_pubkey == out.script_pubkey {
            existing_out.value += out.value;
            already_exists = true;
          }
        }
        if !already_exists {
          merged.output.push(out.clone());
        }
      }
    }

    // TODO: Randomize DER encoding

    // Randomize the input and output order
    self.rng.shuffle(merged.input.as_mut_slice());
    self.rng.shuffle(merged.output.as_mut_slice());

    self.signed = Some(Transaction {
      version: merged.version,
      lock_time: merged.lock_time,
      input: merged.input.iter().map(
        |input| TxIn {
          prev_hash: input.prev_hash,
          prev_index: input.prev_index,
          script_sig: Default::default(),
          sequence: input.sequence
        }).collect(),
      output: merged.output.clone()
    });
    self.merged = Some(merged);
  }

  /// Adds a signed transaction to a coinjoin session
  pub fn add_signed(&mut self, tx: &Transaction, utxo_set: &UtxoSet)
                      -> Result<(), CoinjoinError> {
    if self.state != Merging {
      return Err(IncorrectState(Merging, self.state));
    }
    let merged = self.merged.as_ref().unwrap();
    let signed = self.signed.as_mut().unwrap();

    // Quick sanity checks
    if merged.input.len() != tx.input.len() {
      return Err(WrongInputCount(tx.input.len()));
    }
    if merged.output.len() != tx.output.len() {
      return Err(WrongOutputCount(tx.output.len()));
    }

    // Check that all the right inputs are there, in order
    for (expected, actual) in merged.input.iter().zip(tx.input.iter()) {
      if expected.prev_hash != actual.prev_hash ||
         expected.prev_index != actual.prev_index ||
         expected.sequence != actual.sequence {
        return Err(UnexpectedInput(actual.prev_hash, actual.prev_index as uint));
      }
    }

    // Check that all the right outputs are there, in order
    for (expected, actual) in merged.output.iter().zip(tx.output.iter()) {
      if expected.value != actual.value ||
         expected.script_pubkey != actual.script_pubkey {
        return Err(UnexpectedOutput(actual.script_pubkey.clone(), actual.value));
      }
    }

    // Check that at least one of the inputs validates
    let mut still_needed = 0u;
    let mut n_new_inputs = 0u;
    for (i, input) in tx.input.iter().enumerate() {
      if signed.input[i].script_sig == Default::default() {
        if input.validate(utxo_set, tx, i).is_ok() {
          signed.input.get_mut(i).script_sig = input.script_sig.clone();
          n_new_inputs += 1;
        } else {
          still_needed += 1;
        }
      }
    }
    if n_new_inputs == 0 {
      return Err(NoNewSignedInputs);
    }
    // If there are no more needed inputs, send the tx
    if still_needed == 0 {
      // Build the transaction
      // TODO add to mempool, actually transmit
      self.state = Complete;
    }

    Ok(())
  }

  /// Accessor for the current state
  pub fn state(&self) -> SessionState { self.state }

  /// Accessor for the signed TX
  pub fn signed_transaction<'a>(&'a self) -> Option<&'a Transaction> { self.signed.as_ref() }
}

/// A Coinjoin session manager
pub struct Server {
  sessions: HashMap<SessionId, Box<Session>>,
  current: *mut Session
}

impl Server {
  /// Construct a new session manager
  pub fn new() -> Server {
    Server {
      sessions: HashMap::new(),
      current: RawPtr::null()
    }
  }

  /// Retrieves the current session, or None if there is not one
  pub fn current_session<'a>(&'a self) -> Option<&'a Session> {
    unsafe { self.current.as_ref() }
  }

  /// Retrieves the current session, or None if there is not one
  pub fn current_session_mut<'a>(&'a mut self) -> Option<&'a mut Session> {
    unsafe { if self.current.is_not_null() { Some(&mut *self.current) } else { None } }
  }

  /// Retrieves a specified session, or None if it is not available
  pub fn session<'a>(&'a self, key: &SessionId) -> Option<&'a Session> {
    self.sessions.find(key).map(|r| &**r)
  }

  /// Retrieves a specified session, or None if it is not available
  pub fn session_mut<'a>(&'a mut self, key: &SessionId) -> Option<&'a mut Session> {
    self.sessions.find_mut(key).map(|r| &mut **r)
  }

  /// Sets the current session
  pub fn set_current_session(&mut self, sess: Session) {
    let boxed = box sess;
    let raw = &*boxed as *const _ as *mut _;
    self.sessions.insert(boxed.id, boxed);
    self.current = raw;
  }

  /// Updates all sessions
  pub fn update_all(&mut self) {
    let now = precise_time_ns();

    let mut keys_to_delete = Vec::new();

    // Run through list, updating session states
    for (key, session) in self.sessions.mut_iter() {
      let time_since_switch = Duration::nanoseconds(precise_time_ns() as i64 - session.switch_time as i64);

      match session.state {
        Joining => {
          if time_since_switch > session.join_duration {
            if session.unsigned.len() > 1 {
              session.state = Merging;
              session.merge_transactions();
            } else {
              session.state = Unmerged;
            }
            session.switch_time = now;
          }
        }
        state => {
          if time_since_switch > session.expiry_duration {
            session.state = match state {
              Joining => unreachable!(),
              Merging => Expired,
              Complete | Expired | Failed | Unmerged => { keys_to_delete.push(*key); Expired }
            };
            session.switch_time = now;
          }
        }
      }
    }
    // Delete any old sessions
    for key in keys_to_delete.iter() {
      unsafe {
        if (*self.current).id == *key {
          self.current = RawPtr::null();
        }
      }
      self.sessions.remove(key);
    }
  }
}




