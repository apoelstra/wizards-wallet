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
use bitcoin::network::listener::Listener;
use bitcoin::network::socket::Socket;
use bitcoin::network::message::NetworkMessage;
use bitcoin::network::message;
use bitcoin::network::message_blockdata::{GetHeadersMessage, Inventory, InvBlock};
use bitcoin::util::misc::consume_err;
use bitcoin::util::hash::zero_hash;

/// We use this IdleState structure to avoid having Option<T>
/// on some stuff that isn't available during bootstrap.
struct IdleState {
  sock: Socket,
  net_chan: Receiver<NetworkMessage>,
  blockchain: Blockchain,
  utxo_set: UtxoSet
}

enum StartupState {
  Init,
  LoadFromDisk(Socket, Receiver<NetworkMessage>),
  SyncBlockchain(IdleState),
  SyncUtxoSet(IdleState),
  SaveToDisk(IdleState), 
  Idle(IdleState)
}

pub struct Bitcoind {
  peer_address: String,
  peer_port: u16,
  blockchain_path: Path,
  utxo_set_path: Path
}

macro_rules! with_next_message(
  ( $recv:expr, $( $name:pat => $code:expr )* ) => (
    {
      let mut ret;
      loop {
        match $recv {
          $(
            $name => {
              ret = $code;
              break;
            },
          )*
          _ => {}
        };
      }
      ret
    }
  )
)

impl Bitcoind {
  pub fn new(peer_address: &str, peer_port: u16, blockchain_path: Path, utxo_set_path: Path) -> Bitcoind {
    Bitcoind {
      peer_address: String::from_str(peer_address),
      peer_port: peer_port,
      blockchain_path: blockchain_path,
      utxo_set_path: utxo_set_path
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
          let (chan, sock) = try!(self.start());
          LoadFromDisk(sock, chan)
        }
        // Load cached blockchain and utxo set from disk
        LoadFromDisk(sock, chan) => {
          println!("Loading blockchain...");
          // Load blockchain from disk
          let blockchain = match Serializable::deserialize_file(&self.blockchain_path) {
            Ok(blockchain) => blockchain,
            Err(e) => {
              println!("Failed to load blockchain: {:}, starting from genesis.", e);
              Blockchain::new(genesis_block())
            }
          };
          println!("Loading utxo set...");
          let utxo_set = match Serializable::deserialize_file(&self.utxo_set_path) {
            Ok(utxo_set) => utxo_set,
            Err(e) => {
              println!("Failed to load UTXO set: {:}, starting from genesis.", e);
              UtxoSet::new(genesis_block())
            }
          };

          SyncBlockchain(IdleState {
              sock: sock,
              net_chan: chan,
              blockchain: blockchain,
              utxo_set: utxo_set
            })
        },
        // Synchronize the blockchain with the peer
        SyncBlockchain(mut idle_state) => {
          println!("Headers sync: last best tip {}", idle_state.blockchain.best_tip().header.hash());
          let mut done = false;
          while !done {
            // Request headers
            consume_err("Headers sync: failed to send `headers` message",
              idle_state.sock.send_message(message::GetHeaders(
                  GetHeadersMessage::new(idle_state.blockchain.locator_hashes(),
                                         zero_hash()))));
            // Loop through received headers
            with_next_message!(idle_state.net_chan.recv(),
              message::Headers(headers) => {
                for lone_header in headers.iter() {
                  if !idle_state.blockchain.add_header(lone_header.header) {
                     println!("Headers sync: failed to add {} to chain", lone_header.header.hash());
                  }
                }
                // We are done if this `headers` message did not update our status
                done = headers.len() == 0;
              }
              message::Ping(nonce) => {
                consume_err("Warning: failed to send pong in response to ping",
                  idle_state.sock.send_message(message::Pong(nonce)));
              }
            );
          }
          println!("Done sync.");
          SyncUtxoSet(idle_state)
        },
        SyncUtxoSet(mut idle_state) => {
          let last_hash = idle_state.utxo_set.last_hash();
          println!("utxo set last hash {}", last_hash);
          let mut failed = false;
          // TODO: unwind any reorgs
          // Loop through blockchain for new data
          {
            // Skip first block, since that's the one we already have
            let mut iter = idle_state.blockchain.iter(last_hash);
            let mut skipped_first = false;
            // Get second block
            let mut next_block = iter.next();
            // Process blocks, prefetching the next block before preprocessing this one
            while !failed && next_block.is_some() {
              // Prefetch next block
              let prefetch = iter.next();
              if prefetch.is_some() && !prefetch.unwrap().has_txdata {
                let inv = Inventory { inv_type: InvBlock, hash: prefetch.unwrap().block.header.hash() };
                consume_err("UTXO sync: failed to send `getdata` message",
                  idle_state.sock.send_message(message::GetData(vec![inv])));
              }
              if !skipped_first {
                skipped_first = true;
                continue;
              }
              // Process this one
              if next_block.unwrap().height % 100 == 0 {
                println!("processing block {}", next_block.unwrap().height);
              }
              if next_block.unwrap().has_txdata {
                if !idle_state.utxo_set.update(&next_block.unwrap().block) {
                  println!("Failed to update UTXO set with block {}", next_block.unwrap().block.header.hash());
                  failed = true;
                  break;
                }
              } else {
                let mut got_block = false;
                while !got_block {
                  with_next_message!(idle_state.net_chan.recv(),
                    message::Block(block) => {
                      if block.header.hash() == next_block.unwrap().block.header.hash() {
                        if !idle_state.utxo_set.update(&block) {
                          println!("Failed to update UTXO set with block {}", next_block.unwrap().block.header.hash());
                          failed = true;
                        }
                        // ignore any other block messages for now
                        got_block = true;
                      }
                    }
                    message::NotFound(_) => {
                      println!("UTXO sync: received `notfound` from sync peer, failing sync.");
                      failed = true;
                      got_block = true;
                    }
                    message::Ping(nonce) => {
                      consume_err("Warning: failed to send pong in response to ping",
                        idle_state.sock.send_message(message::Pong(nonce)));
                    }
                  )
                }
              }
              next_block = prefetch;
            }
          }
          // TODO: save last 100 blocks
          if failed {
            println!("Failed to sync UTXO set, trying to resync chain.");
            SyncBlockchain(idle_state)
          } else {
            SaveToDisk(idle_state)
          }
        },
        // Idle loop
        Idle(mut idle_state) => {
          println!("Idling...");
          let recv = idle_state.net_chan.recv();
          idle_message(&mut idle_state, recv);
          Idle(idle_state)
        },
        // Temporary states
        SaveToDisk(idle_state) => {
          println!("Saving blockchain...");
          match idle_state.blockchain.serialize_file(&self.blockchain_path) {
            Ok(()) => { println!("Successfully saved blockchain.") },
            Err(e) => { println!("failed to write blockchain: {:}", e); }
          }
          println!("Saving UTXO set...");
          match idle_state.utxo_set.serialize_file(&self.utxo_set_path) {
            Ok(()) => { println!("Successfully saved UTXO set.") },
            Err(e) => { println!("failed to write UTXO set: {:}", e); }
          }
          Idle(idle_state)
        }
      };
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

/// Idle message handler
fn idle_message(idle_state: &mut IdleState, message: NetworkMessage) {
  match message {
    message::Version(_) => {
      // TODO: actually read version message
      consume_err("Warning: failed to send getdata in response to inv",
        idle_state.sock.send_message(message::Verack));
    }
    message::Verack => {}
    message::Block(block) => {
      println!("Received block: {:x}", block.header.hash());
      if !idle_state.blockchain.add_header(block.header) {
        println!("failed to add block {:x} to chain", block.header.hash());
      }
    },
    message::Headers(headers) => {
      for lone_header in headers.iter() {
        println!("Received header: {}, ignoring.", lone_header.header.hash());
      }
    },
    message::Inv(inv) => {
      println!("Received inv.");
      let sendmsg = message::GetData(inv);
      // Send
      consume_err("Warning: failed to send getdata in response to inv",
        idle_state.sock.send_message(sendmsg));
    }
    message::GetData(_) => {}
    message::NotFound(_) => {}
    message::GetBlocks(_) => {}
    message::GetHeaders(_) => {}
    message::Ping(nonce) => {
      consume_err("Warning: failed to send pong in response to ping",
        idle_state.sock.send_message(message::Pong(nonce)));
    }
    message::Pong(_) => {}
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

