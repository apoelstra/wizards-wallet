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
use std::num::from_str_radix;
use std::io::IoResult;
use std::rand::Rng;
use std::time::Duration;
use num::integer::div_mod_floor;
use serialize::json;
use serialize::{Decodable, Decoder, Encodable, Encoder};
use time::precise_time_ns;

/// Current state of the session
#[deriving(PartialEq, Eq, PartialOrd, Ord, Show)]
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
  Failed
}

impl json::ToJson for SessionState {
  fn to_json(&self) -> json::Json {
    json::String(match *self {
      Joining => "joining",
      Merging => "merging",
      Complete => "complete",
      Expired => "expired",
      Failed => "failed"
    }.to_string())
  }
}

/// A Coinjoin session
#[deriving(PartialEq, Eq, Show)]
pub struct Session {
  id: SessionId,
  state: SessionState,
  // Time at which last state switch occured
  switch_time: u64,
  // Duration of "collecting unsigned transactions" phase
  join_duration: Duration,
  // Duration of every other phase before we expire or delete the session
  expiry_duration: Duration,
  target_value: u64
}

impl json::ToJson for Session {
  fn to_json(&self) -> json::Json {
    let (secs, nanos) = div_mod_floor(precise_time_ns() - self.switch_time, 1_000_000_000);
    let time_since_switch = Duration::seconds(secs as i32) + Duration::nanoseconds(nanos as i32);

    let mut obj = TreeMap::new();
    obj.insert("id".to_string(), self.id.to_json());
    obj.insert("state".to_string(), self.state.to_json());
    obj.insert("join_duration".to_string(), self.join_duration.num_milliseconds().to_json());
    obj.insert("merge_duration".to_string(), self.expiry_duration.num_milliseconds().to_json());
    if self.state == Joining {
      obj.insert("time_until_merge".to_string(),
                 (self.join_duration + time_since_switch).num_milliseconds().to_json());
    } else {
      obj.insert("time_until_expiry".to_string(),
                 (self.expiry_duration + time_since_switch).num_milliseconds().to_json());
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
      None    => Err(d.error("Session ID was not a valid hex string"))
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
             expiry_duration: Duration)
             -> IoResult<Session> {
    use std::rand;
    let mut rng = try!(rand::OsRng::new());
    let id = SessionId(rng.gen());
    Ok(Session {
      id: id,
      target_value: target_value,
      state: Joining,
      switch_time: precise_time_ns(),
      join_duration: join_duration,
      expiry_duration: expiry_duration
    })
  }

  /// Retrieves the immutable ID of the session
  pub fn id(&self) -> SessionId {
    self.id
  }
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
    unsafe { self.current.to_option() }
  }

  /// Retrieves a specified session, or None if it is not available
  pub fn session<'a>(&'a self, key: &SessionId) -> Option<&'a Session> {
    self.sessions.find(key).map(|r| &**r)
  }

  /// Sets the current session
  pub fn set_current_session(&mut self, sess: Session) {
    let boxed = box sess;
    let raw = &*boxed as *const _ as *mut _;
    self.sessions.insert(sess.id, boxed);
    self.current = raw;
  }

  /// Updates all sessions
  pub fn update_all(&mut self) {
    let now = precise_time_ns();

    let mut keys_to_delete = Vec::new();

    // Run through list, updating session states
    for (key, session) in self.sessions.mut_iter() {
      let (secs, nanos) = div_mod_floor(now - session.switch_time, 1_000_000_000);
      let time_since_switch = Duration::seconds(secs as i32) + Duration::nanoseconds(nanos as i32);

      match session.state {
        Joining => {
          if time_since_switch > session.join_duration {
            session.state = Merging;
            session.switch_time = now;
          }
        }
        state => {
          if time_since_switch > session.expiry_duration {
            session.state = match state {
              Joining => unreachable!(),
              Merging => Expired,
              Complete | Expired | Failed => { keys_to_delete.push(*key); Expired }
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




