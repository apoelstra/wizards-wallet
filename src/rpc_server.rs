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
use std::time::Duration;
use serialize::Decodable;
use serialize::json;
use serialize::json::ToJson;

use bitcoin::util::hash::Sha256dHash;
use jsonrpc;
use jsonrpc::error::{standard_error, Error, InvalidParams, MethodNotFound};
use phf::PhfOrderedMap;

use bitcoind::IdleState;
use coinjoin::server::{Server, Session, SessionId};

pub type JsonResult = jsonrpc::JsonResult<json::Json>;

/// A single RPC command
pub struct RpcCall {
  name: &'static str,
  desc: &'static str,
  usage: &'static str,
  coinjoin: bool,
  call: fn(&RpcCall, &mut IdleState, Vec<json::Json>) -> JsonResult
}

// Forget you saw this macro...just forget it.
macro_rules! rpc_calls(
  ( $( #[doc=$doc:tt]
       #[usage=$usage:tt]
       #[coinjoin=$coinjoin:tt]
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
            coinjoin: $coinjoin,
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
  #[coinjoin=false]
  pub fn help(_: &RpcCall, idle_state: &mut IdleState, _: Vec<json::Json>) {
    let mut ret = TreeMap::new();
    for call in RPC_CALLS.values() {
      if !call.coinjoin || idle_state.config.coinjoin_on {
        let mut obj = TreeMap::new();
        obj.insert("description".to_string(), json::String(call.desc.to_string()));
        obj.insert("usage".to_string(), json::String(call.usage.to_string()));
        ret.insert(call.name.to_string(), json::Object(obj));
      }
    }
    Ok(json::Object(ret))
  },

  #[doc="Gets a specific block from the blockchain"]
  #[usage="<hash>"]
  #[coinjoin=false]
  pub fn getblock(rpc: &RpcCall, idle_state: &mut IdleState, params: Vec<json::Json>) {
    match params.len() {
      1 => {
        let blockchain = idle_state.blockchain.read();
        let hash: Sha256dHash = try!(decode_param(params[0].clone()));

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
      _ => Err(usage_error(rpc))
    }
  },

  #[doc="Gets the current number of unspent outputs on the blockchain."]
  #[usage=""]
  #[coinjoin=false]
  pub fn getutxocount(rpc: &RpcCall, idle_state: &mut IdleState, params: Vec<json::Json>) {
    match params.len() {
      0 => Ok(json::Number(idle_state.utxo_set.read().n_utxos() as f64)),
      _ => Err(usage_error(rpc))
    }
  },

  #[doc="Gets the length of the longest chain, starting from the given hash or genesis."]
  #[usage="[start hash]"]
  #[coinjoin=false]
  pub fn getblockcount(rpc: &RpcCall, idle_state: &mut IdleState, params: Vec<json::Json>) {
    match params.len() {
      0 => {
        let blockchain = idle_state.blockchain.read();
        // Subtract 1 from the hash since the genesis counts as block 0
        Ok(json::Number(blockchain.iter(blockchain.genesis_hash()).count() as f64 - 1.0))
      }
      1 => {
        let blockchain = idle_state.blockchain.read();
        let hash: Sha256dHash = try!(decode_param(params[0].clone()));

        // Subtract 1 from the hash since the genesis counts as block 0
        match blockchain.iter(hash).count() {
          0 => Err(bitcoin_json_error(BlockNotFound, Some(hash.to_json()))),
          n => Ok(json::Number(n as f64 - 1.0)),
        }
      }
      _ => Err(usage_error(rpc))
    }
  },

  #[doc="Starts a new coinjoin session"]
  #[usage="<target amount (satoshi)> <join duration (seconds)> <merge duration (seconds)>"]
  #[coinjoin=true]
  pub fn coinjoin_start(rpc: &RpcCall, idle_state: &mut IdleState, params: Vec<json::Json>) { 
    match params.len() {
      3 => {
        let target: u64 = try!(decode_param(params[0].clone()));
        let join_duration = Duration::milliseconds(try!(decode_param(params[1].clone())));
        let expiry_duration = Duration::milliseconds(try!(decode_param(params[2].clone())));

        // Start session manager if we haven't
        if idle_state.coinjoin.is_none() {
          idle_state.coinjoin = Some(Server::new());
        }
        // Update the server state
        let server = idle_state.coinjoin.get_mut_ref();
        server.update_all();
        // Add the new sesion
        let session = try!(Session::new(target, join_duration, expiry_duration)
                             .map_err(|e| bitcoin_json_error(BadRng,
                                                             Some(json::String(e.to_string())))));
        let id = session.id();
        server.set_current_session(session);
        Ok(id.to_json())
      }
      _ => Err(usage_error(rpc))
    }
  },

  #[doc="Gets the status of the current coinjoin session"]
  #[usage="[session id]"]
  #[coinjoin=true]
  pub fn coinjoin_status(rpc: &RpcCall, idle_state: &mut IdleState, params: Vec<json::Json>) {
    if idle_state.coinjoin.is_none() {
      return Err(bitcoin_json_error(SessionNotFound, None));
    }
    // Update the server state
    let server = idle_state.coinjoin.get_mut_ref();
    server.update_all();

    match params.len() {
      0 => server.current_session().map_or(Err(bitcoin_json_error(SessionNotFound, None)), |s| Ok(s.to_json())),
      1 => {
        let id: SessionId = try!(decode_param(params[0].clone()));
        server.session(&id).map_or(Err(bitcoin_json_error(SessionNotFound, None)), |s| Ok(s.to_json()))
      }
      _ => Err(usage_error(rpc))
    }
  }
}

enum BitcoinJsonError {
  BadRng,
  BlockNotFound,
  SessionNotFound
}

/// Decode a Json parameter
fn decode_param<T:Decodable<json::Decoder, json::DecoderError>>(param: json::Json) -> jsonrpc::JsonResult<T> {
  let mut decoder = json::Decoder::new(param);
  Decodable::decode(&mut decoder)
    .map_err(|e| standard_error(InvalidParams,
                                Some(json::String(e.to_string()))))
}

/// Create a standard error responses
fn bitcoin_json_error(code: BitcoinJsonError, data: Option<json::Json>) -> Error {
  match code {
    BadRng => Error {
      code: -1,
      message: "Bad RNG".to_string(),
      data: data
    },
    BlockNotFound => Error {
      code: -2,
      message: "Block not found".to_string(),
      data: data
    },
    SessionNotFound => Error {
      code: -3,
      message: "Coinjoin session not found".to_string(),
      data: data
    }
  }
}

/// Generates a `usage` error message
fn usage_error(rpc: &RpcCall) -> Error {
  standard_error(InvalidParams,
                 Some(json::String(format!("Usage: {} {}", rpc.name, rpc.usage))))
}

/// Handles a JSON-RPC request, returning a result to be given back to the peer
pub fn handle_rpc(request: jsonrpc::Request, idle_state: &mut IdleState) -> JsonResult {
  let method = request.method.as_slice();
  match RPC_CALLS.find_equiv(&method) {
    Some(rpc) if !rpc.coinjoin || idle_state.config.coinjoin_on =>
      (rpc.call)(rpc, idle_state, request.params),
    _ => Err(standard_error(MethodNotFound,
                            Some(json::String(request.method.clone()))))
  }
}

