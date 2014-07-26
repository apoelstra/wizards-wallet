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

use serialize::json;

use bitcoind::IdleState;
use jsonrpc;
use phf::PhfMap;

type JsonResult = jsonrpc::JsonResult<json::Json>;

struct RpcCall {
  name: &'static str,
  call: fn(&mut IdleState) -> JsonResult
}

static RPC_CALLS: PhfMap<&'static str, RpcCall> = phf_map! {
  "getutxocount" => {
    fn getutxocount(idle_state: &mut IdleState) -> JsonResult {
      Ok(json::Number(idle_state.utxo_set.read().n_utxos() as f64))
    }
    RpcCall { name: "getutxocount", call: getutxocount }
  },
};

pub fn handle_rpc(request: jsonrpc::Request, idle_state: &mut IdleState) -> JsonResult {
  let method = request.method.as_slice();
  match RPC_CALLS.find_equiv(&method) {
    Some(rpc) => (rpc.call)(idle_state),
    None => Err(jsonrpc::error::standard_error(jsonrpc::error::MethodNotFound,
                                               Some(json::String(request.method.clone()))))
  }
}

