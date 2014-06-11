
use extra::json::{Json,Number,Object,String};
use extra::treemap::TreeMap;
use extra::url::Url;
use http::client::RequestWriter;
use http::method::Post;
use std::io::net::ip::SocketAddr;
use std::io::net::tcp::TcpStream;
use std::str;

struct JsonRpc {
  priv url: Url,
  priv req_id: uint
}

impl JsonRpc {
  /**
   * Create a new JSON-RPC object
   */
  pub fn new(url: &str) -> Option<JsonRpc>
  {
    let u: Option<Url> = from_str (url);
    match u {
      Some(u) => Some(JsonRpc { url: u, req_id: 0 }),
      None => None
    }
  }

  /**
   * Private ``get request ID'' fn
   */
  fn get_request_id (&mut self) -> f64 {
    self.req_id += 1;
    self.req_id as f64
  }

  /**
   * Send a JSON-RPC requenst
   */
  pub fn request (&mut self, method: &str, params: Option<Json>) -> Option<Json>
  {
    let mut request_contents = ~TreeMap::new();

    request_contents.insert (~"id", Number(self.get_request_id()));
    request_contents.insert (~"jsonrpc", String(~"2.0"));
    request_contents.insert (~"method", String(method.to_owned()));
    match params {
      Some(js) => {
        request_contents.insert (~"params", js);
      }
      None => {}
    }

    /* Send the request */
    let request: RequestWriter<TcpStream> = RequestWriter::new (Post, self.url.clone());
    let request_str = Object(request_contents).to_str();

    /* Blocking read the reply */
    println ("Reading...");
    let mut reply = match request.read_response() {
      Ok(reply) => reply,
      Err(_) => { return None; }
    };

    let body = reply.read_to_end();
    println(str::from_utf8(body));

    None
  }
}


