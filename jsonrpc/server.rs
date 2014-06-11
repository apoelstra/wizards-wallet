/* Coinjoin Server
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

use http::server::{Config, Server, Request, ResponseWriter};
use http::headers::content_type::MediaType;

use std::io::net::ip::{SocketAddr, Ipv4Addr};

use collections::treemap::TreeMap;
use serialize::json::{Json,Object,from_str};
use time;

#[deriving(Clone)]
pub struct JsonRpcServer {
  req_tx: Sender<(Json, Sender<(Json, Json)>)>,
}


/* Object stuff */
pub fn new (req_tx: Sender<(Json, Sender<(Json, Json)>)>) -> JsonRpcServer
{
  let rv = JsonRpcServer {
    req_tx: req_tx,
  };
  rv
}

/* Server implementation */
impl Server for JsonRpcServer {
  fn get_config (&self) -> Config
  {
      Config { bind_address: SocketAddr { ip: Ipv4Addr(127, 0, 0, 1), port: 8001 } }
  }

  fn handle_request (&self, r: &Request, w: &mut ResponseWriter)
  {
    w.headers.date = Some (time::now_utc());
    w.headers.content_type = Some (MediaType {
      type_: StrBuf::from_str ("application"),
      subtype: StrBuf::from_str ("json"),
      parameters: vec![(StrBuf::from_str ("charset"), StrBuf:: from_str ("UTF-8"))]
    });
    w.headers.server = Some (StrBuf::from_str ("coinjoin-server"));

    match from_str (r.body.as_slice()) {
      Ok(js) => {
        /* Check that the message is an actual jsonrpc request and get its ID */
        let id_json = match js {
          Object(ref obj) => {
            match obj.find (&"id".to_owned()) {
              Some(i) => i.clone(),
              _ => {
                match w.write (format! ("\\{\"error\": \"JSONRPC request has no id.\"\\}").as_bytes()) {
                  Ok(_) => {}
                  Err(e) => { println! ("Stream IO Error: {:s}", e.desc); }
                }
                return;
              }
            }
          }
          _ => {
            match w.write (format! ("\\{\"error\": \"JSON appears not to be an RPC request.\"\\}").as_bytes()) {
              Ok(_) => {}
              Err(e) => { println! ("Stream IO Error: {:s}", e.desc); }
            }
            return;
          }
        };

        /* Send the result back to the caller for processing, get its response. */
        let (resp_tx, resp_rx) = channel();
        self.req_tx.send ((js, resp_tx));
        let (result, error) = resp_rx.recv();

        /* Format it and pass it along */
        let mut reply_obj = TreeMap::new();
        reply_obj.insert("result".to_owned(), result);
        reply_obj.insert("error".to_owned(), error);
        reply_obj.insert("id".to_owned(), id_json);
        let reply_json = Object(box reply_obj);
        let reply_str = reply_json.to_str();
        let reply_bytes = reply_str.as_bytes();

        w.headers.content_length = Some (reply_bytes.len());
        match w.write (reply_bytes) {
          Ok(_) => {}
          Err(e) => { println! ("Stream IO Error: {:s}.", e.desc); }
        }
      }
      Err(e) => {
        println!("error {:s} ````{:s}''''", e.to_str(), r.body);
        match w.write (format! ("\\{\"error\": \"{:s}\"\\}", e.to_str()).as_bytes()) {
          Ok(_) => {}
          Err(e) => { println! ("Stream IO Error: {:s}.", e.desc); }
        }
      }
    }
  }
}

