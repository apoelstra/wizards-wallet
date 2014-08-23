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

use bitcoin::blockdata::transaction::TransactionError;
use bitcoin::util::hash::Sha256dHash;

pub mod server;

/// A Coinjoin-related error
#[deriving(Clone, PartialEq, Eq, Show)]
pub enum CoinjoinError {
  /// Transactions could not be merged
  BadMerge(TransactionError),
  /// Tx had an input which already appears in the join
  DuplicateInput(Sha256dHash, uint),
  /// Tx had a nonzero locktime
  NonZeroLocktime(uint),
  /// Tx had no output of the target size (target in sat)
  NoTargetOutput(u64),
  /// Tx had an input which the joiner did not know about
  UnknownInput(Sha256dHash, uint),
  /// Tx had a version which the joiner did not understand
  UnknownVersion(uint)
}


