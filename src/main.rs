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
extern crate xdg;

use std::io::timer;

use bitcoind::Bitcoind;
use jsonrpc::server::JsonRpcServer;
use http::server::Server;
use user_data::{blockchain_path, utxo_set_path};

mod bitcoind;
mod constants;
mod rpc_server;
mod user_data;

/// Entry point
fn main()
{
  println!("Starting the Wizards' Wallet");

  // Connect to bitcoind
  let network = bitcoin::network::constants::Bitcoin;
  let (jsonrpc, rpc_rx) = JsonRpcServer::new();
  let mut bitcoind = Bitcoind::new("127.0.0.1", 8333,
                                   network, rpc_rx,
                                   blockchain_path(network),
                                   utxo_set_path(network));
  // Spawn the RPC server
  spawn (proc() {
    println!("Starting JSON RPC server...");
    jsonrpc.serve_forever();
    println!("JSON RPC server shut down.");
  });
  // Loop until we get a successful connection
  loop {
    match bitcoind.listen() {
      Err(e) => {
        println!("Got error {:}, trying to connect again...", e);
        timer::sleep(1000);
      }
      _ => { break; }
    }
  }
}




