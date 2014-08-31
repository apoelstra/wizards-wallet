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

//! # The Wizards' Wallet
//!
//! The Wizards' Wallet is a SPV Bitcoin Wallet designed for ease of prototyping
//! and a willingness to experiment with user interfaces, and exposing potentially
//! dangerous or experimental ideas built on top of the Bitcoin protocol.
//!
//! It is also written entirely in Rust to illustrate the benefits of strong type
//! safety, including ownership and lifetime, for financial and/or cryptographic
//! software.
//!


#![crate_name = "wizards-wallet"]

#![comment = "The Wizards' Wallet"]
#![license = "CC0"]

// Experimental features we need
#![feature(globs)]
#![feature(phase)]
#![feature(macro_rules)]

// Coding conventions
#![warn(non_uppercase_statics)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![warn(missing_doc)]

extern crate num;
extern crate rand;
extern crate rustrt;
extern crate serialize;
extern crate sync;
extern crate time;

#[phase(plugin,link)] extern crate bitcoin;
extern crate crypto = "rust-crypto";
extern crate http;
extern crate jsonrpc;
#[phase(plugin)] extern crate phf_mac;
extern crate phf;
extern crate toml;
extern crate xdg;

#[cfg(not(test))]
use bitcoind::Bitcoind;
#[cfg(not(test))]
use jsonrpc::server::JsonRpcServer;
#[cfg(not(test))]
use http::server::Server;
#[cfg(not(test))]
use user_data::{config_path, load_configuration};
// Public exports to get documentation
pub mod bitcoind;
pub mod coinjoin;
pub mod constants;
pub mod rpc_server;
pub mod user_data;

/// Entry point
#[cfg(not(test))]
fn main()
{
  println!("Starting the Wizards' Wallet");

  let config = match load_configuration(&config_path()) {
      Some(config) => config,
      None => { println!("Failed to load configuration. Shutting down."); return; }
    };

  for config in config.move_iter() {
    let network = config.network;
    println!("main: Starting a listener for {}", network);
    // Connect to bitcoind
    let (jsonrpc, rpc_rx) = match JsonRpcServer::new(config.rpc_server_addr.as_slice(),
                                                     config.rpc_server_port) {
      Err(e) => {
        println!("{}: RPC server: {}, failed to start.", network, e);
        break;
      }
      Ok(tup) => tup
    };
    // Start bitcoind
    let bitcoind = Bitcoind::new(config, rpc_rx);
    spawn(proc() {
      let mut bitcoind = bitcoind;
      match bitcoind.listen() {
        Err(e) => {
          println!("{}: Got error {:}, failed to start.", network, e);
        }
        _ => {}
      }
    });
    // Start the RPC server
    spawn (proc() {
      println!("{}: Starting JSON RPC server...", network);
      jsonrpc.serve_forever();
      println!("{}: JSON RPC server shut down.", network);
    });
  }
  println!("main: started all networks");
}

