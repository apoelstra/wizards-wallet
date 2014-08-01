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
#![deny(non_uppercase_pattern_statics)]
#![deny(uppercase_variables)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case_functions)]
#![deny(unused_mut)]
#![warn(missing_doc)]

extern crate rand;
extern crate rustrt;
extern crate sync;
extern crate time;
extern crate serialize;

#[phase(plugin,link)] extern crate bitcoin;
extern crate http;
extern crate jsonrpc;
#[phase(plugin)] extern crate phf_mac;
extern crate phf;
extern crate toml;
extern crate xdg;

use bitcoind::Bitcoind;
use jsonrpc::server::JsonRpcServer;
use http::server::Server;
use user_data::{config_path, load_configuration};

// Public exports to get documentation
pub mod bitcoind;
pub mod coinjoin;
pub mod constants;
pub mod rpc_server;
pub mod user_data;

/// Entry point
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
    let (jsonrpc, rpc_rx) = JsonRpcServer::new();
    let bitcoind = Bitcoind::new(config, rpc_rx);
    spawn(proc() {
      let mut bitcoind = bitcoind;
      match bitcoind.listen() {
        Err(e) => {
          println!("{}: Got error {:}, failed to listen.", network, e);
        }
        _ => {
          // If we got a bitcoind up, start the RPC server
          spawn (proc() {
            println!("{}: Starting JSON RPC server...", network);
            jsonrpc.serve_forever();
            println!("{}: JSON RPC server shut down.", network);
          });
        }
      }
    });
  }
  println!("main: started all networks");
}

