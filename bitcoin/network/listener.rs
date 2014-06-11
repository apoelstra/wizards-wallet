// Rust Bitcoin Library
// Written in 2014 by
//   Andrew Poelstra <apoelstra@wpsoftware.net>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

use std::io::{IoResult, standard_error, ConnectionFailed};
use std::io::timer;

use util::hash::zero_hash;
use network::serialize::Message;
use network::message_network::VersionAckMessage;
use network::message_blockdata::GetBlocksMessage;
use network::socket::Socket;
use network::constants;

enum StateMachine {
  Init,
  Handshaking,
  GettingBlocks
}

/// A message which can be sent on the Bitcoin network
pub trait Listener {
  fn peer<'a>(&'a self) -> &'a str;
  fn port(&self) -> u16;
  /// Main listen loop
  fn start(&self) -> IoResult<Sender<Box<Message : Send>>> {
    // Open socket
    let mut new_sock = Socket::new(constants::MAGIC_BITCOIN);
    match new_sock.connect(self.peer(), self.port()) {
      Ok(_) => {},
      Err(_) => return Err(standard_error(ConnectionFailed))
    }

    let (sendmsg_tx, sendmsg_rx) = channel::<Box<Message : Send>>();


    // Send version message to peer
    let version_message = try!(new_sock.version_message(0));
    try!(new_sock.send_message(&version_message));

    // Message loop
    spawn(proc() {
      let mut handshake_complete = false;
      loop {
        let mut sock = new_sock;
        // Send any messages as appropriate
        let mut to_send = sendmsg_rx.try_recv();
        while to_send.is_ok() {
          let sendmsg = to_send.ok().unwrap();
          sock.send_message(sendmsg);
          to_send = sendmsg_rx.try_recv();
        }

        // Receive new message
        match sock.receive_message() {
          Ok(msg) => {
            match msg.command.as_slice() {
              "verack" => {
                // TODO: when the timeout stuff in std::io::net::tcp is sorted out we should
                // actually time out if the verack doesn't come in in time
                if handshake_complete {
                  println!("Received second verack (peer is misbehaving)");
                } else {
                  handshake_complete = true;
                }
              }
              "version" => {
                // TODO: we should react to the version data
                println!("received {:s}", msg.command);
                println!("received {:}", msg.data.as_slice());
                sock.send_message(&VersionAckMessage::new());
              }
              s => {
                println!("Received unknown message type {:s}", s);
              }
            }
          }
          Err(e) => {
            println!("Received error {:}, trying again in 1s.", e);
            timer::sleep(1000);
          }
        }
      }
    });
    Ok(sendmsg_tx)
  }
}



