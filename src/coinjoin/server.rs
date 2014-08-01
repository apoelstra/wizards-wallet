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

use std::collections::HashMap;
use std::num::from_str_radix;
use std::io::IoResult;
use std::rand::Rng;
use serialize::{Decodable, Decoder, Encodable, Encoder};

/// A Coinjoin session
pub struct Session {
  id: SessionId,
  target_value: u64
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

impl Session {
  /// Creates a new session with a random ID
  pub fn new(target_value: u64) -> IoResult<Session> {
    use std::rand;
    let mut rng = try!(rand::OsRng::new());
    let id = SessionId(rng.gen());
    Ok(Session {
      id: id,
      target_value: target_value
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

  /// Sets the current session
  pub fn set_current_session(&mut self, sess: Session) {
    let boxed = box sess;
    let raw = &*boxed as *const _ as *mut _;
    self.sessions.insert(sess.id, boxed);
    self.current = raw;
  }
}




