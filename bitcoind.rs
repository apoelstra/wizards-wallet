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

use std::io::{IoError, IoResult, IoUnavailable};
use std::comm::Select;

use bitcoin::blockdata::block::BlockHeader;
use bitcoin::blockdata::blockchain::Blockchain;
use bitcoin::blockdata::constants::genesis_block;
use bitcoin::network::serialize::Serializable;
use bitcoin::network::listener::{Listener, ListenerChannels};
use bitcoin::network::socket::Socket;
use bitcoin::network::message_blockdata::{GetDataMessage, GetHeadersMessage};
use bitcoin::util::misc::consume_err;
use bitcoin::util::hash::zero_hash;

use user_data;

pub struct Bitcoind {
  peer_address: String,
  peer_port: u16,
  blockchain: Blockchain,
  channels: Option<ListenerChannels>,
  sock: Option<Socket>,
  last_best_tip: Option<BlockHeader>
}

impl Bitcoind {
  pub fn new(peer_address: &str, peer_port: u16, blockchain_path: &Path) -> Bitcoind {
    Bitcoind {
      peer_address: String::from_str(peer_address),
      peer_port: peer_port,
      // Load blockchain from disk
      blockchain: match Serializable::deserialize_file(blockchain_path) {
        Ok(blockchain) => {
  let blockchain: Blockchain = blockchain;
println!("Read blockchain, best tip {:x}", blockchain.best_tip().hash());
  blockchain },
        Err(e) => {
          println!("Failed to load blockchain: {:}, starting from genesis.", e);
          Blockchain::new(genesis_block().header)
        }
      },
      channels: None,
      sock: None,
      last_best_tip: None
    }
  }

  /// Sends a `getheaders` message; the `headers` handler will do the rest
  pub fn sync_blockchain(&mut self) -> IoResult<()> {
    println!("Starting sync.");
    match self.sock {
      Some(ref mut sock) => {
        try!(sock.send_message(&GetHeadersMessage::new(self.blockchain.locator_hashes(), zero_hash())));
        Ok(())
      },
      None => Err(IoError {
        kind: IoUnavailable,
        desc: "cannot sync channel -- nowhere to send messages",
        detail: None
      }),
    }
  }

  pub fn listen(&mut self) -> IoResult<()> {
    // Open socket
    let (ch, sk) = try!(self.start());
    self.channels = Some(ch);
    self.sock = Some(sk);
    // Sync with chain
    try!(self.sync_blockchain());
    // Listen for messages
    // note that this is a manual unwrapping of the select! macro in std/macros.rs
    // See #12902 https://github.com/rust-lang/rust/issues/12902 for why this is necessary.
    let sel = Select::new();
    let block_ref = &self.channels.get_ref().block_rx;
    let mut block_h = sel.handle(block_ref);
    let header_ref = &self.channels.get_ref().header_rx;
    let mut header_h = sel.handle(header_ref);
    let inv_ref = &self.channels.get_ref().inv_rx;
    let mut inv_h = sel.handle(inv_ref);
    unsafe {
      block_h.add();
      header_h.add();
      inv_h.add();
    }
    // This loop never returns
    loop {
      let id = sel.wait();
      if id == block_h.id() {
        let block = block_h.recv();
        println!("Received block: {:x}", block.header.hash());
        if !self.blockchain.add_header(block.header) {
          println!("failed to add block {:x} to chain", block.header.hash());
        }
      } else if id == header_h.id() {
        let header_opt = header_h.recv();
        match header_opt {
          Some(header) => {
            if !self.blockchain.add_header(*header) {
              println!("failed to add block {:x} to chain", header.hash());
            }
          }
          // None is code for `end of headers message`
          None => {
            let new_best_tip = self.blockchain.best_tip();
            if self.last_best_tip.is_none() ||
               self.last_best_tip.get_ref() != self.blockchain.best_tip() {
              consume_err("Warning: failed to send headers message",
                self.sock.get_mut_ref().send_message(&GetHeadersMessage::new(self.blockchain.locator_hashes(), zero_hash())));
            } else {
              println!("Done sync.");
              match self.blockchain.serialize_file(&user_data::blockchain_path()) {
                Ok(()) => { println!("Successfully saved blockchain.") },
                Err(e) => { println!("failed to write blockchain: {:}", e); }
              }
            }
            self.last_best_tip = Some(*new_best_tip.clone());
          }
        }
      } else if id == inv_h.id() {
        let data = inv_h.recv();
        let sendmsg = GetDataMessage(data);
        // Send
        consume_err("Warning: failed to send getdata in response to inv",
          self.sock.get_mut_ref().send_message(&sendmsg));
      }
    }
  }
}

impl Listener for Bitcoind {
  fn peer<'a>(&'a self) -> &'a str {
    self.peer_address.as_slice()
  }

  fn port(&self) -> u16 {
    self.peer_port
  }
}

#[cfg(test)]
mod tests {
  use bitcoin::network::listener::Listener;

  use bitcoind::Bitcoind;

  #[test]
  fn test_bitcoind() {
    let bitcoind = Bitcoind::new("localhost", 1000);
    assert_eq!(bitcoind.peer(), "localhost");
    assert_eq!(bitcoind.port(), 1000);

    let mut bitcoind = Bitcoind::new("127.0.0.1", 0);
    assert!(bitcoind.listen().is_err());
  }
}

