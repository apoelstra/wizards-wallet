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

//! # Abstract Bitcoin listener
//!
//! This module defines a listener on the Bitcoin network which is able
//! to connect to a peer, send network messages, and receive Bitcoin data.
//!

use std::io::{IoResult, standard_error, ConnectionFailed};
use std::io::timer;
use sync::comm::{Handle, Select};

use blockdata::block::{Block, BlockHeader};
use network::serialize::{Serializable, Message};
use network::message_network::{VersionAckMessage, PingMessage, PongMessage};
use network::message_blockdata::{InventoryMessage, Inventory, HeadersMessage};
use network::socket::Socket;
use network::constants;

// Everything ListenerChannels-related is a huge mess, waiting on
// #12902 with a sane interface
struct PrivListenerChannels {
  sel: Select,
  block_rxh: Handle<'static, Box<Block>>,
  header_rxh: Handle<'static, Option<Box<BlockHeader>>>,
  inv_rxh: Handle<'static, Vec<Inventory>>,
}

#[unsafe_destructor]
impl Drop for PrivListenerChannels {
  // We have to do this before self.sel is dropped
  fn drop(&mut self) {
    unsafe {
      self.block_rxh.remove();
      self.header_rxh.remove();
      self.inv_rxh.remove();
    }
  }
}

/// Container for communication channels with the listening thread
pub struct ListenerChannels {
  priv_lc: Box<PrivListenerChannels>,
  /// Receiver for new blocks received by peer
  pub block_rx: Receiver<Box<Block>>,
  /// Receiver for new blockheaders received by peer
  pub header_rx: Receiver<Option<Box<BlockHeader>>>,
  /// Receiver for new inv messages received by peer
  pub inv_rx: Receiver<Vec<Inventory>>
}

pub enum RecvMessages {
  RecvBlock(Box<Block>),
  RecvHeader(Option<Box<BlockHeader>>),
  RecvInv(Vec<Inventory>),
}

impl ListenerChannels {
  pub fn recv(&mut self) -> RecvMessages {
    let id = self.priv_lc.sel.wait();
    if id == self.priv_lc.block_rxh.id() {
      RecvBlock(self.priv_lc.block_rxh.recv())
    }
    else if id == self.priv_lc.header_rxh.id() {
      RecvHeader(self.priv_lc.header_rxh.recv())
    }
    else if id == self.priv_lc.inv_rxh.id() {
      RecvInv(self.priv_lc.inv_rxh.recv())
    }
    else { fail!("Bug 153055"); }
  }
}

/// A message which can be sent on the Bitcoin network
pub trait Listener {
  /// Return a string encoding of the peer's network address
  fn peer<'a>(&'a self) -> &'a str;
  /// Return the port we have connected to the peer on
  fn port(&self) -> u16;
  /// Main listen loop
  fn start(&self) -> IoResult<(Box<ListenerChannels>, Socket)> {
    // Open socket
    let mut ret_sock = Socket::new(constants::MAGIC_BITCOIN);
    match ret_sock.connect(self.peer(), self.port()) {
      Ok(_) => {},
      Err(_) => return Err(standard_error(ConnectionFailed))
    }
    let mut sock = ret_sock.clone();

    let (block_tx, block_rx) = channel();
    let (header_tx, header_rx) = channel();
    let (inv_tx, inv_rx) = channel();

    // Send version message to peer
    let version_message = try!(sock.version_message(0));
    try!(sock.send_message(&version_message));

    // Message loop
    spawn(proc() {
      let mut handshake_complete = false;
      let mut sock = sock;
      loop {
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
                match sock.send_message(&VersionAckMessage::new()) {
                  Err(e) => {
                    println!("Warning: error sending verack: {:}", e);
                  },
                  _ => {}
                }
              }
              "inv" => {
                // TDOO: we should filter the inv message instead of just requesting all the data
                let msg_decode: IoResult<InventoryMessage> = Serializable::deserialize(msg.data.iter().map(|n| *n));
                match msg_decode {
                  Ok(msg) => {
                    // Tranlate inv to getdata
                    let InventoryMessage(data) = msg;
                    inv_tx.send(data);
                  }
                  Err(e) => {
                    println!("Warning: received error decoding inv: {:}", e);
                  }
                }
              }
              "block" => {
                let block_decode: IoResult<Block> = Serializable::deserialize(msg.data.iter().map(|n| *n));
                match block_decode {
                  Ok(block) => {
                    block_tx.send(box block);
                  }
                  Err(e) => {
                    println!("Warning: received error decoding block: {:}", e);
                  }
                }
              }
              "headers" => {
                let msg_decode: IoResult<HeadersMessage> = Serializable::deserialize(msg.data.iter().map(|n| *n));
                match msg_decode {
                  Ok(headers) => {
                    let HeadersMessage(data) = headers;
                    for header in data.move_iter() {
                      // header will be a LoneBlockHeader, which has an extraneous tx_count
                      // field (which is zero anyway). header.header is the actual BlockHeader.
                      header_tx.send(Some(box header.header));
                    }
                    header_tx.send(None);
                  }
                  Err(e) => {
                    println!("Warning: received error decoding headers: {:}", e);
                  }
                }
              }
              // Ping
              "ping" => {
                let msg_decode: IoResult<PingMessage> = Serializable::deserialize(msg.data.iter().map(|n| *n));
                match msg_decode {
                  Ok(ping) => {
                    let PingMessage { nonce: nonce } = ping;
                    match sock.send_message(&PongMessage { nonce: nonce }) {
                       Err(e) => {
                        println!("Warning: error sending pong: {:}", e);
                      },
                      _ => {}
                    }
                  }
                  Err(e) => {
                    println!("Warning: received error decoding ping: {:}", e);
                  }
                }
              }
              // Unknown message
              s => {
                println!("Received unknown message type {:s}", s);
              }
            }
          }
          Err(e) => {
            println!("Received error {:} when decoding message.", e);
            timer::sleep(1000);
          }
        }
      }
    });
    // Set up ListenerChannels
    let mut ret_channels = box ListenerChannels { 
      // Set `sel` into place, but leave the handles uninitialized since
      // they depend on `sel` not moving once they are set.
      priv_lc: unsafe { 
        use std::mem::uninitialized;
        box PrivListenerChannels {
          sel: Select::new(),
          block_rxh: uninitialized(),
          header_rxh: uninitialized(),
          inv_rxh: uninitialized()
        }
      },
      block_rx: block_rx,
      header_rx: header_rx,
      inv_rx: inv_rx
    };
    // Set handles in place
    unsafe {
      use std::mem::transmute;
      // Cast everything to static. Fuck borrowck.
      let stat_sel: &'static Select = transmute(&ret_channels.priv_lc.sel);
      let stat_block_rx: &'static Receiver<Box<Block>> = transmute(&ret_channels.block_rx);
      let stat_header_rx: &'static Receiver<Option<Box<BlockHeader>>> = transmute(&ret_channels.header_rx);
      let stat_inv_rx: &'static Receiver<Vec<Inventory>> = transmute(&ret_channels.inv_rx);
      ret_channels.priv_lc.block_rxh = stat_sel.handle(stat_block_rx);
      ret_channels.priv_lc.header_rxh = stat_sel.handle(stat_header_rx);
      ret_channels.priv_lc.inv_rxh = stat_sel.handle(stat_inv_rx);
      ret_channels.priv_lc.block_rxh.add();
      ret_channels.priv_lc.header_rxh.add();
      ret_channels.priv_lc.inv_rxh.add();
    }

    Ok((ret_channels, ret_sock))
  }
}



