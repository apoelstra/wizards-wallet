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

//! # RPC Server
//!
//! Functions and data to handle RPC calls

use std::collections::TreeMap;
use serialize::Decodable;
use serialize::json;
use serialize::json::ToJson;

use bitcoin::util::hash::Sha256dHash;
use jsonrpc;
use jsonrpc::error::{standard_error, Error, InvalidParams, MethodNotFound};
use phf::PhfOrderedMap;

use bitcoind::IdleState;

pub type JsonResult = jsonrpc::JsonResult<json::Json>;

/// A single RPC command
pub struct RpcCall {
  name: &'static str,
  desc: &'static str,
  usage: &'static str,
  call: fn(&RpcCall, &mut IdleState, Vec<json::Json>) -> JsonResult
}

// Forget you saw this macro...just forget it.
macro_rules! rpc_calls(
  ( $( #[doc=$doc:tt]
       #[usage=$usage:tt]
       pub fn $name:ident($($param:tt: $paramty:ty),+) $code:expr),+ ) => (
    $(
      // `tt` token trees can only be passed to a macro. On the other hand,
      // there is no other type which will accept a doccomment as a token.
      // So we accept a tt and hand it to the macro_rules! macro ;)
      //
      // Notice that we are using the same name as the outer macro...this
      // means that the macro can only be called once, since we effectively
      // overwrite it here.
      macro_rules! rpc_calls( () => (
        // I don't really want these to be public, but they don't show up
        // in the output of `cargo doc` otherwise.
        #[doc=$doc]
        pub fn $name($($param: $paramty),+) -> JsonResult { $code }
      ))
      rpc_calls!()
    )+
    // Let's do it again!
    macro_rules! rpc_calls( () => (
      static RPC_CALLS: PhfOrderedMap<&'static str, RpcCall> = phf_ordered_map! {
        $(stringify!($name) => RpcCall {
            name: stringify!($name),
            desc: $doc,
            usage: $usage,
            call: $name
          }
        ),+
      };
    ))
    rpc_calls!()
    // Erase the dummy macro, to avoid confusing errors in case somebody
    // tries to use it outside of this macro.
    macro_rules! rpc_calls(
      () => (Sorry, you can only call rpc_calls! once.)
    )
  )
)

// Main RPC call list
rpc_calls!{
  #[doc="Fetches a list of commands"]
  #[usage=""]
  pub fn help(_: &RpcCall, _: &mut IdleState, _: Vec<json::Json>) {
    let mut ret = TreeMap::new();
    for call in RPC_CALLS.values() {
      let mut obj = TreeMap::new();
      obj.insert("description".to_string(), json::String(call.desc.to_string()));
      obj.insert("usage".to_string(), json::String(call.usage.to_string()));
      ret.insert(call.name.to_string(), json::Object(obj));
    }
    Ok(json::Object(ret))
  },

  #[doc="Gets a specific block from the blockchain"]
  #[usage="<hash>"]
  pub fn getblock(rpc: &RpcCall, idle_state: &mut IdleState, params: Vec<json::Json>) {
    match params.len() {
      1 => {
        let blockchain = idle_state.blockchain.read();
        // Read the hash
        let mut decoder = json::Decoder::new(params[0].clone());
        let hash: Sha256dHash = try!(Decodable::decode(&mut decoder)
                                       .map_err(|e| standard_error(InvalidParams,
                                                                   Some(json::String(e.to_string())))));

        match blockchain.get_block(hash) {
          Some(node) => {
            let mut ret = TreeMap::new();
            ret.insert("header".to_string(), node.block.header.to_json());
            ret.insert("has_txdata".to_string(), json::Boolean(node.has_txdata));
            if node.has_txdata {
              ret.insert("transactions".to_string(), node.block.txdata.to_json());
            }
            Ok(json::Object(ret))
          }
          None => Err(bitcoin_json_error(BlockNotFound, Some(hash.to_json()))),
        }
      }
      _ => usage_error(rpc)
    }
  },

  #[doc="Gets the current number of unspent outputs on the blockchain."]
  #[usage=""]
  pub fn getutxocount(rpc: &RpcCall, idle_state: &mut IdleState, params: Vec<json::Json>) {
    match params.len() {
      0 => Ok(json::Number(idle_state.utxo_set.read().n_utxos() as f64)),
      _ => usage_error(rpc)
    }
  },

  #[doc="Gets the length of the longest chain, starting from the given hash or genesis."]
  #[usage="[start hash]"]
  pub fn getblockcount(rpc: &RpcCall, idle_state: &mut IdleState, params: Vec<json::Json>) {
    match params.len() {
      0 => {
        let blockchain = idle_state.blockchain.read();
        // Subtract 1 from the hash since the genesis counts as block 0
        Ok(json::Number(blockchain.iter(blockchain.genesis_hash()).count() as f64 - 1.0))
      }
      1 => {
        let blockchain = idle_state.blockchain.read();
        // Read the hash
        let mut decoder = json::Decoder::new(params[0].clone());
        let hash: Sha256dHash = try!(Decodable::decode(&mut decoder)
                                       .map_err(|e| standard_error(InvalidParams,
                                                                   Some(json::String(e.to_string())))));

        // Subtract 1 from the hash since the genesis counts as block 0
        match blockchain.iter(hash).count() {
          0 => Err(bitcoin_json_error(BlockNotFound, Some(hash.to_json()))),
          n => Ok(json::Number(n as f64 - 1.0)),
        }
      }
      _ => usage_error(rpc)
    }
  }
}

enum BitcoinJsonError {
  BlockNotFound
}

/// Create a standard error responses
fn bitcoin_json_error(code: BitcoinJsonError, data: Option<json::Json>) -> Error {
  match code {
    BlockNotFound => Error {
      code: -1,
      message: "Block not found".to_string(),
      data: data
    }
  }
}

/// Generates a `usage` error message
fn usage_error(rpc: &RpcCall) -> JsonResult {
  Err(standard_error(InvalidParams,
                     Some(json::String(format!("Usage: {} {}", rpc.name, rpc.usage)))))
}

/// Handles a JSON-RPC request, returning a result to be given back to the peer
pub fn handle_rpc(request: jsonrpc::Request, idle_state: &mut IdleState) -> JsonResult {
  let method = request.method.as_slice();
  match RPC_CALLS.find_equiv(&method) {
    Some(rpc) => (rpc.call)(rpc, idle_state, request.params),
    None => Err(standard_error(MethodNotFound,
                               Some(json::String(request.method.clone()))))
  }
}

