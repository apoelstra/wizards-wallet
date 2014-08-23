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
//! Functions and data to join transactions and to manage a centralized
//! coinjoin server.

use bitcoin::util::hash::Sha256dHash;
use bitcoin::blockdata::script::Script;

use self::server::SessionState;

pub mod server;

/// A Coinjoin-related error
#[deriving(Clone, PartialEq, Eq, Show)]
pub enum CoinjoinError {
  /// Tx had an input which already appears in the join
  DuplicateInput(Sha256dHash, uint),
  /// Session is in the wrong state for this action (actual, expected)
  IncorrectState(SessionState, SessionState),
  /// Signed TX did not actually introduce new signed inputs
  NoNewSignedInputs,
  /// Tx had a nonzero locktime
  NonZeroLocktime(uint),
  /// Tx had no output of the target size (target in sat)
  NoTargetOutput(u64),
  /// Tx total output value exceed the total input value
  OutputsExceedInputs(u64, u64),
  /// Signed tx had an input that was not the expected one
  UnexpectedInput(Sha256dHash, uint),
  /// Signed tx had an output that was not the expected one
  UnexpectedOutput(Script, u64),
  /// Tx had an input which the joiner did not know about
  UnknownInput(Sha256dHash, uint),
  /// Tx had a version which the joiner did not understand
  UnknownVersion(uint),
  /// Signed tx had too many inputs
  WrongInputCount(uint),
  /// Signed tx had too many outputs
  WrongOutputCount(uint)
}


