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

use std::io::{InvalidInput, IoError, IoResult};
use std::io::{BufferedReader, BufferedWriter, File, Open, Write};
use std::str;
use std::rand::{mod, Rng};
use serialize::Decodable;

use toml;
use bitcoin::wallet::bip32;
use bitcoin::wallet::wallet::Wallet;
use bitcoin::network::constants::Network;

use user_data::NetworkConfig;

/// Attempts to load a wallet from disk
pub fn load_wallet(config: &NetworkConfig) -> IoResult<Wallet> {
  let mut file = BufferedReader::new(try!(File::open(&config.wallet_path)));
  let data = try!(file.read_to_end());
  let str_data = str::from_utf8(data.as_slice());
  if str_data.is_none() {
    return Err(IoError { kind: InvalidInput,
                         desc: "wallet file was not UTF-8", 
                         detail: None });
  }
  let str_data = str_data.unwrap();

  let mut parser = toml::Parser::new(str_data.as_slice());
  match parser.parse() {

    Some(table) => {
      let mut d = toml::Decoder::new(toml::Table(table));
      Decodable::decode(&mut d).map_err(|e| IoError {
        kind: InvalidInput,
        desc: "wallet TOML did not parse to wallet",
        detail: Some(format!("{}", e))
      })
    }
    None => Err(IoError {
      kind: InvalidInput,
      desc: "could not parse wallet TOML",
      detail: Some(format!("{}", parser.errors))
    })
  }
}

/// Saves a wallet to disk
pub fn save_wallet(config: &NetworkConfig, wallet: &Wallet) -> IoResult<()> {
  let mut file = BufferedWriter::new(try!(File::open_mode(&config.wallet_path, Open, Write)));
  let data = toml::encode_str(wallet);
  file.write_str(data.as_slice())
}

/// Creates a new default wallet
pub fn default_wallet(network: Network) -> Result<Wallet, bip32::Error> {
  let mut rng = try!(rand::OsRng::new().map_err(|e| bip32::RngError(format!("{}", e))));
  let mut seed = [0, ..256];
  rng.fill_bytes(seed.as_mut_slice());
  Wallet::from_seed(network, seed.as_slice())
}


