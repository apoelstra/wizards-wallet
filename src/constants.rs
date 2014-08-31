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

//! # Constants
//!
//! Defines compile-time constants which determine operation of the wallet.
//! As a general rule, anything in here ought to be a user-configurable
//! option, and is on the TODO list.
//!

/// The number of blocks to request at once during UTXO sync
pub static UTXO_SYNC_N_BLOCKS: uint = 500;

/// The number of blocks to store full blockdata on in case of reorg
pub static BLOCKCHAIN_N_FULL_BLOCKS: uint = 100;

/// The save-to-disk frequency in s
pub static SAVE_FREQUENCY: i64 = 600; // 10 minutes

/// Default peer address
pub static DEFAULT_PEER_ADDR: &'static str = "localhost";

/// Default peer port
pub static DEFAULT_PEER_PORT: u16 = 8333;

/// Default RPC server address
pub static DEFAULT_RPC_SERVER_ADDR: &'static str = "localhost";

/// Default RPC server port
pub static DEFAULT_RPC_SERVER_PORT: u16 = 8001;

