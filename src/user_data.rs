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

//! # User data handling
//!
//! Functions for storing and reading data from disk are here
//!

use xdg;
use std::path::posix::Path;

use bitcoin::network::constants::{Network, Bitcoin, BitcoinTestnet};

/// Returns a path to the blockchain file on disk
pub fn blockchain_path(network: Network) -> Path {
  let dirs = xdg::XdgDirs::new();
  match network {
    Bitcoin => dirs.want_write_cache("wizards-wallet/blockchain.bitcoin.dat"),
    BitcoinTestnet => dirs.want_write_cache("wizards-wallet/blockchain.testnet.dat")
  }
}

/// Returns a path to the UTXO cache on disk
pub fn utxo_set_path(network: Network) -> Path {
  let dirs = xdg::XdgDirs::new();
  match network {
    Bitcoin => dirs.want_write_cache("wizards-wallet/utxoset.bitcoin.dat"),
    BitcoinTestnet => dirs.want_write_cache("wizards-wallet/utxoset.testnet.dat")
  }
}


