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
use std::collections::hashmap::Entries;
use std::default::Default;
use std::io::{File, IoResult, IoError, InvalidInput, FileNotFound};
use std::path::posix::Path;
use std::str::from_utf8;
use serialize::{Decoder, Encoder, Decodable, Encodable};

use xdg;

use bitcoin::network::constants::{Network, Bitcoin, BitcoinTestnet};

/// Returns a path to the user's configuration file on disk
pub fn config_path() -> Path {
  let dirs = xdg::XdgDirs::new();
  dirs.want_write_config("wizards-wallet/wizards-wallet.conf")
}

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

/// User's global program configuration for a specific network
#[deriving(Encodable, Show)]
pub struct NetworkConfig {
  /// Address to connect to the network peer on
  pub peer_addr: String,
  /// Port to connect to the network peer on
  pub peer_port: u16,
  /// Address to listen for RPC requests on
  pub rpc_server_addr: String,
  /// Port to listen for RPC requests on
  pub rpc_server_port: u16
}

impl<E, D:Decoder<E>> Decodable<D, E> for NetworkConfig {
  fn decode(d: &mut D) -> Result<NetworkConfig, E> {
    use constants::DEFAULT_PEER_ADDR;
    use constants::DEFAULT_PEER_PORT;
    use constants::DEFAULT_RPC_SERVER_ADDR;
    use constants::DEFAULT_RPC_SERVER_PORT;

    let peer_addr: Option<String> = try!(d.read_struct_field("peer_addr", 0u,
                                                             Decodable::decode));
    let peer_port: Option<u16> = try!(d.read_struct_field("peer_port", 1u,
                                                          Decodable::decode));
    let rpc_server_addr: Option<String> = try!(d.read_struct_field("rpc_server_addr", 2u,
                                                                   Decodable::decode));
    let rpc_server_port: Option<u16> = try!(d.read_struct_field("rpc_server_port", 3u,
                                                                Decodable::decode));

    Ok(NetworkConfig {
      peer_addr: peer_addr.unwrap_or(DEFAULT_PEER_ADDR.to_string()),
      peer_port: peer_port.unwrap_or(DEFAULT_PEER_PORT),
      rpc_server_addr: rpc_server_addr.unwrap_or(DEFAULT_RPC_SERVER_ADDR.to_string()),
      rpc_server_port: rpc_server_port.unwrap_or(DEFAULT_RPC_SERVER_PORT)
    })
  }
}

#[deriving(Show)]
/// A list of user configuration for all networks
pub struct Config(HashMap<Network, NetworkConfig>);

impl Config {
  /// Returns a (key, value) iterator over all networks and their configurations
  pub fn iter<'a>(&'a self) -> Entries<'a, Network, NetworkConfig> {
    let &Config(ref data) = self;
    data.iter()
  }
}

impl Default for Config {
  fn default() -> Config {
    let mut ret = HashMap::new();
    ret.insert(Bitcoin, NetworkConfig {
        peer_addr: "localhost".to_string(),
        peer_port: 8333,
        rpc_server_addr: "localhost".to_string(),
        rpc_server_port: 8001
      });
    Config(ret)
  }
}

impl<E, D:Decoder<E>> Decodable<D, E> for Config {
  fn decode(d: &mut D) -> Result<Config, E> {
    Ok(Config(try!(Decodable::decode(d))))
  }
}

impl<E, S:Encoder<E>> Encodable<S, E> for Config {
  fn encode(&self, s: &mut S) -> Result<(), E> {
    let &Config(ref data) = self;
    data.encode(s)
  }
}

fn read_configuration(path: &Path) -> IoResult<Config> {
  use serialize::Decodable;
  use toml::{Parser, Decoder, Table};

  let mut config_file = try!(File::open(path));
  let config_data = try!(config_file.read_to_end());
  let contents = from_utf8(config_data.as_slice());

  match contents {
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
println!("table {}", table);
      let mut d = Decoder::new(Table(table));
      let res = Decodable::decode(&mut d);
println!("res {}", res);
      res.map_err(|err| IoError {
          kind: InvalidInput,
          desc: "TOML parser error",
          detail: Some(err.to_string())
        })
    },
    None => Err(IoError {
      kind: InvalidInput,
      desc: "Configuration file must be valid UTF8",
      detail: None
    })
  }
}

/// Parses a configuration file and returns its bounty
pub fn load_configuration(path: &Path) -> Option<Config> {
  // Try to parse the user's config file
  match read_configuration(path) {
    Ok(res) => {
      println!("got config: {}", res);
      Some(res)
    },
    Err(err) => {
      // For file not found, we use the default configuration...
      if err.kind == FileNotFound {
        println!("Did not find {}, using default configuration.", path.display());
        Some(Default::default())
      }
      // But for anything else, the user must've made a mistake. Better to do nothing.
      else {
        println!("{}", err);
        None
      }
    }
  }
}

