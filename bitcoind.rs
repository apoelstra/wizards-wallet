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

use std::io::IoResult;
use std::path::posix::Path;

use bitcoin::blockdata::blockchain::Blockchain;
use bitcoin::blockdata::constants::genesis_block;
use bitcoin::blockdata::utxoset::UtxoSet;
use bitcoin::network::serialize::Serializable;
use bitcoin::network::listener::{Listener, ListenerChannels};
use bitcoin::network::socket::Socket;
use bitcoin::network::message_blockdata::{GetDataMessage, GetHeadersMessage};
use bitcoin::util::misc::consume_err;
use bitcoin::util::hash::{Sha256dHash, zero_hash};

struct IdleState<'a> {
  sock: Socket,
  chan: ListenerChannels<'a>,
  blockchain: Blockchain,
}

enum StartupState<'a> {
  Init,
  LoadFromDisk(Socket, ListenerChannels<'a>),
  SyncBlockchain(Socket, ListenerChannels<'a>, Blockchain, Sha256dHash),
//  SyncUtxoSet(uint),            // height
  SaveBlockchain(Box<StartupState<'a>>), // next state
  Idle(IdleState<'a>)
}

pub struct Bitcoind {
  peer_address: String,
  peer_port: u16,
  blockchain_path: Path
}

impl Bitcoind {
  pub fn new(peer_address: &str, peer_port: u16, blockchain_path: Path) -> Bitcoind {
    Bitcoind {
      peer_address: String::from_str(peer_address),
      peer_port: peer_port,
      blockchain_path: blockchain_path
    }
  }

  /// Run the state machine
  pub fn listen(&mut self) -> IoResult<()> {
    let mut state = Init;
    // Eternal state machine loop
    loop {
      state = match state {
        // First startup
        Init => {
          // Open socket
          let (channels, sock) = try!(self.start());
          LoadFromDisk(sock, channels)
        }
        // Load cached blockchain and utxoset from disk
        LoadFromDisk(sock, channels) => {
          println!("Loading blockchain...");
          // Load blockchain from disk
          let blockchain = match Serializable::deserialize_file(&self.blockchain_path) {
            Ok(blockchain) => blockchain,
            Err(e) => {
              println!("Failed to load blockchain: {:}, starting from genesis.", e);
              Blockchain::new(genesis_block())
            }
          };
          let best_tip_hash = blockchain.best_tip().header.hash();
          SyncBlockchain(sock, channels, blockchain, best_tip_hash)
        },
        // Synchronize the blockchain with the peer
        SyncBlockchain(mut sock, channels, mut blockchain, last_best_tip_hash) => {
          println!("Headers sync: last best tip {}", last_best_tip_hash);
          // Request headers
          consume_err("Headers sync: failed to send `headers` message",
            sock.send_message(&GetHeadersMessage::new(blockchain.locator_hashes(), zero_hash())));
          // Loop through received headers
          loop {
            match channels.header_rx.recv() {
              // Each `headers` message is passed to us as a None-terminated list of headers
              None => break,
              Some(header) => {
                if !blockchain.add_header(*header) {
                  println!("Headers sync: failed to add {} to chain", header.hash());
                }
              }
            }
          }
          // Check if we are done sync'ing
          let new_best_tip_hash = blockchain.best_tip().header.hash();
          if new_best_tip_hash != last_best_tip_hash {
            SyncBlockchain(sock, channels, blockchain, new_best_tip_hash)
          } else {
            println!("Done sync.");
            SaveBlockchain(box Idle(IdleState {
              sock: sock,
              chan: channels,
              blockchain: blockchain
            }))
          }
        },
        // Idle loop
        Idle(mut idle_state) => {
          println!("Idling...");
          nu_select!{
            block from idle_state.chan.block_rx => {
              println!("Received block: {:x}", block.header.hash());
              if !idle_state.blockchain.add_header(block.header) {
                println!("failed to add block {:x} to chain", block.header.hash());
              }
            },
            header_opt from idle_state.chan.header_rx => {
              println!("Received header: {}", header_opt.as_ref().map(|h| h.hash()));
              match header_opt {
                Some(header) => {
                  if !idle_state.blockchain.add_header(*header) {
                    println!("failed to add block {:x} to chain", header.hash());
                  }
                }
                None => {}
              }
            },
            inv from idle_state.chan.inv_rx => {
              println!("Received inv.");
              let sendmsg = GetDataMessage(inv);
              // Send
              consume_err("Warning: failed to send getdata in response to inv",
                idle_state.sock.send_message(&sendmsg));
            }
          }
          Idle(idle_state)
        }
        // Temporary states
        SaveBlockchain(box next_state) => {
          match next_state {
            Idle(ref idle_state) => {
              println!("Saving blockchain...");
              match idle_state.blockchain.serialize_file(&self.blockchain_path) {
                Ok(()) => { println!("Successfully saved blockchain.") },
                Err(e) => { println!("failed to write blockchain: {:}", e); }
              }
            },
            _ => {
              println!("Warn: tried to save blockchain in non-idle state. Refusing.");
            }
          }
          next_state
        }
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

  use user_data::blockchain_path;
  use bitcoind::Bitcoind;

  #[test]
  fn test_bitcoind() {
    let bitcoind = Bitcoind::new("localhost", 1000, &blockchain_path());
    assert_eq!(bitcoind.peer(), "localhost");
    assert_eq!(bitcoind.port(), 1000);

    let mut bitcoind = Bitcoind::new("localhost", 0, &blockchain_path());
    assert!(bitcoind.listen().is_err());
  }
}

