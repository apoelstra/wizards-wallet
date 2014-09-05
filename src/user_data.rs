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

use std::collections::HashMap;
use std::io::{File, IoResult, IoError, InvalidInput, FileNotFound};
use std::path::posix::Path;
use std::str::from_utf8;
use std::vec::MoveItems;
use serialize::Decoder;

use xdg;

use bitcoin::network::constants::{Network, Bitcoin, BitcoinTestnet};

use bitcoind::{DebugLevel, Status};

/// Returns the path to the user's configuration file on disk
pub fn config_path() -> Path {
  let dirs = xdg::XdgDirs::new();
  dirs.want_write_config("wizards-wallet/wizards-wallet.conf")
}

/// Returns the default path to the blockchain file on disk
fn blockchain_path(network: Network) -> Path {
  let dirs = xdg::XdgDirs::new();
  match network {
    Bitcoin => dirs.want_write_cache("wizards-wallet/blockchain.bitcoin.dat"),
    BitcoinTestnet => dirs.want_write_cache("wizards-wallet/blockchain.testnet.dat")
  }
}

/// Returns the default path to the UTXO cache on disk
fn utxo_set_path(network: Network) -> Path {
  let dirs = xdg::XdgDirs::new();
  match network {
    Bitcoin => dirs.want_write_cache("wizards-wallet/utxoset.bitcoin.dat"),
    BitcoinTestnet => dirs.want_write_cache("wizards-wallet/utxoset.testnet.dat")
  }
}

/// Returns the default path to the user's wallet file on disk
fn wallet_path(network: Network) -> Path {
  let dirs = xdg::XdgDirs::new();
  match network {
    Bitcoin => dirs.want_write_config("wizards-wallet/wallet.bitcoin.toml"),
    BitcoinTestnet => dirs.want_write_config("wizards-wallet/wallet.testnet.toml")
  }
}

/// User's global program configuration for a specific network
#[deriving(Clone)]
pub struct NetworkConfig {
  /// The network this configuration is for
  pub network: Network,
  /// Address to connect to the network peer on
  pub peer_addr: String,
  /// Port to connect to the network peer on
  pub peer_port: u16,
  /// Address to listen for RPC requests on
  pub rpc_server_addr: String,
  /// Port to listen for RPC requests on
  pub rpc_server_port: u16,
  /// Whether to operate a coinjoin server as part of RPC
  pub coinjoin_on: bool,
  /// Path to the on-disk blockchain cache
  pub blockchain_path: Path,
  /// Path to the on-disk UTXO set cache
  pub utxo_set_path: Path,
  /// Path to the user's wallet
  pub wallet_path: Path,
  /// Path to the on-disk UTXO set cache
  pub debug_level: DebugLevel
}

#[deriving(Decodable)]
struct TomlNetworkConfig {
  peer_addr: Option<String>,
  peer_port: Option<u16>,
  rpc_server_addr: Option<String>,
  rpc_server_port: Option<u16>,
  coinjoin_on: Option<bool>,
  blockchain_path: Option<Path>,
  utxo_set_path: Option<Path>,
  wallet_path: Option<Path>,
  debug_level: Option<DebugLevel>
}

/// A list of user configuration for all networks
pub struct Config(Vec<NetworkConfig>);

type TomlConfig = HashMap<Network, TomlNetworkConfig>;

impl Config {
  /// Returns a (key, value) iterator over all networks and their configurations
  pub fn move_iter(self) -> MoveItems<NetworkConfig> {
    let Config(data) = self;
    data.move_iter()
  }
}

fn read_configuration(path: &Path) -> IoResult<Config> {
  use serialize::Decodable;
  use toml::{Parser, Decoder, Table};

  let mut config_file = try!(File::open(path));
  let config_data = try!(config_file.read_to_end());
  let contents = from_utf8(config_data.as_slice());

  // Translate the Toml into a hashmap
  let decode: TomlConfig = match contents {
    Some(contents) => {
      let mut parser = Parser::new(contents.as_slice());
      let table = match parser.parse() {
        Some(table) => table,
        None => {
          let mut error_str = path.display().to_string();
          for error in parser.errors.iter() {
            let (ln_low,  col_low)  = parser.to_linecol(error.lo);
            let (ln_high, col_high) = parser.to_linecol(error.hi);
            error_str.push_str(if ln_low == ln_high && col_low == col_high {
                                 format!("{}:{}:", ln_low + 1, col_low + 1)
                               } else {
                                 format!("{}:{}-{}:{}:", ln_low + 1, col_low + 1,
                                                          ln_high + 1, col_high + 1)
                               }.as_slice());
            error_str.push_str(format!(" {}\n", error.desc).as_slice());
          }
          return Err(IoError {
            kind: InvalidInput,
            desc: "Failed to parse configuration file",
            detail: Some(error_str)
          });
        }
      };
      let mut d = Decoder::new(Table(table));
      let res = Decodable::decode(&mut d);
      try!(res.map_err(|err| IoError {
          kind: InvalidInput,
          desc: "TOML parser error",
          detail: Some(err.to_string())
        }))
    },
    None => {
      return Err(IoError {
        kind: InvalidInput,
        desc: "Configuration file must be valid UTF8",
        detail: None
      });
    }
  };

  // Move the hashmap into something nicer to use, with missing fields
  // filled in by defaults
  let mut ret = Vec::with_capacity(decode.len());
  for (network, toml_config) in decode.move_iter() {
    use constants::DEFAULT_PEER_ADDR;
    use constants::DEFAULT_PEER_PORT;
    use constants::DEFAULT_RPC_SERVER_ADDR;
    use constants::DEFAULT_RPC_SERVER_PORT;

    ret.push(NetworkConfig {
      network: network,
      peer_addr: toml_config.peer_addr.unwrap_or(DEFAULT_PEER_ADDR.to_string()),
      peer_port: toml_config.peer_port.unwrap_or(DEFAULT_PEER_PORT),
      rpc_server_addr: toml_config.rpc_server_addr.unwrap_or(DEFAULT_RPC_SERVER_ADDR.to_string()),
      rpc_server_port: toml_config.rpc_server_port.unwrap_or(DEFAULT_RPC_SERVER_PORT),
      coinjoin_on: toml_config.coinjoin_on.unwrap_or(false),
      blockchain_path: toml_config.blockchain_path.unwrap_or(blockchain_path(network)),
      utxo_set_path: toml_config.utxo_set_path.unwrap_or(utxo_set_path(network)),
      wallet_path: toml_config.wallet_path.unwrap_or(wallet_path(network)),
      debug_level: toml_config.debug_level.unwrap_or(Status)
    });
  }
  Ok(Config(ret))
}

/// Parses a configuration file and returns its bounty
pub fn load_configuration(path: &Path) -> Option<Config> {
  // Try to parse the user's config file
  match read_configuration(path) {
    Ok(res) => Some(res),
    Err(err) => {
      // For file not found, we use the default configuration...
      if err.kind == FileNotFound {
        use constants::DEFAULT_PEER_ADDR;
        use constants::DEFAULT_PEER_PORT;
        use constants::DEFAULT_RPC_SERVER_ADDR;
        use constants::DEFAULT_RPC_SERVER_PORT;

        println!("Did not find {}, using default configuration.", path.display());

        Some(Config(vec![
          NetworkConfig {
            network: Bitcoin,
            peer_addr: DEFAULT_PEER_ADDR.to_string(),
            peer_port: DEFAULT_PEER_PORT,
            rpc_server_addr: DEFAULT_RPC_SERVER_ADDR.to_string(),
            rpc_server_port: DEFAULT_RPC_SERVER_PORT,
            coinjoin_on: false,
            blockchain_path: blockchain_path(Bitcoin),
            utxo_set_path: utxo_set_path(Bitcoin),
            wallet_path: wallet_path(Bitcoin),
            debug_level: Status
          }]))
      }
      // But for anything else, the user must've made a mistake. Better to do nothing.
      else {
        println!("{}", err);
        None
      }
    }
  }
}

