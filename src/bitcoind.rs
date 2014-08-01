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

//! # Bitcoin Daemon
//!
//! Main network listener and idle loop.

use std::io::IoResult;
use std::io::timer::Timer;
use std::sync::{Arc, RWLock};
use serialize::json;

use jsonrpc;

use bitcoin::blockdata::blockchain::Blockchain;
use bitcoin::blockdata::utxoset::UtxoSet;
use bitcoin::network::constants::Network;
use bitcoin::network::serialize::Serializable;
use bitcoin::network::listener::Listener;
use bitcoin::network::socket::Socket;
use bitcoin::network::message::NetworkMessage;
use bitcoin::network::message;
use bitcoin::network::message_blockdata::{GetBlocksMessage, GetHeadersMessage, Inventory, InvBlock};
use bitcoin::util::patricia_tree::PatriciaTree;
use bitcoin::util::misc::consume_err;
use bitcoin::util::hash::zero_hash;

use constants::BLOCKCHAIN_N_FULL_BLOCKS;
use constants::UTXO_SYNC_N_BLOCKS;
use constants::SAVE_FREQUENCY;
use rpc_server::handle_rpc;
use user_data::NetworkConfig;

/// Data used by an idling wallet. This is constructed piecemeal during
/// startup.
pub struct IdleState {
  sock: Socket,
  net_chan: Receiver<NetworkMessage>,
  /// Mutex for blockchain access
  pub blockchain: Arc<RWLock<Blockchain>>,
  /// Mutex for UTXO set access
  pub utxo_set: Arc<RWLock<UtxoSet>>
}

enum StartupState {
  Init,
  LoadFromDisk(Socket, Receiver<NetworkMessage>),
  SyncBlockchain(IdleState),
  SyncUtxoSet(IdleState, Vec<Inventory>),
  SaveToDisk(IdleState), 
  Idle(IdleState)
}

/// The main Bitcoin network listener structure
pub struct Bitcoind {
  /// Configuration for this network
  config: NetworkConfig,
  /// Receiver on which RPC commands come in
  rpc_rx: Receiver<(jsonrpc::Request, Sender<jsonrpc::JsonResult<json::Json>>)>,
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
  /// Constructor
  pub fn new(config: NetworkConfig,
             rpc_rx: Receiver<(jsonrpc::Request, Sender<jsonrpc::JsonResult<json::Json>>)>)
             -> Bitcoind {
    Bitcoind {
      config: config,
      rpc_rx: rpc_rx
    }
  }

  /// Run the state machine
  pub fn listen(&mut self) -> IoResult<()> {
    let mut timer = Timer::new().unwrap();  // TODO: can this fail? what should we do?
    let save_timer = timer.periodic(SAVE_FREQUENCY as u64);
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
          println!("{}: Loading blockchain...", self.config.network);
          // Load blockchain from disk
          let blockchain = match Serializable::deserialize_file(&self.config.blockchain_path) {
            Ok(blockchain) => blockchain,
            Err(e) => {
              println!("{}: Failed to load blockchain: {:}, starting from genesis.", self.config.network, e);
              Blockchain::new(self.config.network)
            }
          };
          println!("{}: Loading utxo set...", self.config.network);
          let utxo_set = match Serializable::deserialize_file(&self.config.utxo_set_path) {
            Ok(utxo_set) => utxo_set,
            Err(e) => {
              println!("{}: Failed to load UTXO set: {:}, starting from genesis.", self.config.network, e);
              UtxoSet::new(self.config.network, BLOCKCHAIN_N_FULL_BLOCKS)
            }
          };

          SyncBlockchain(IdleState {
              sock: sock,
              net_chan: chan,
              blockchain: Arc::new(RWLock::new(blockchain)),
              utxo_set: Arc::new(RWLock::new(utxo_set))
            })
        },
        // Synchronize the blockchain with the peer
        SyncBlockchain(mut idle_state) => {

          // Do a headers-first sync of all blocks
          let mut done = false;
          while !done {
            // Borrow the blockchain mutably
            let mut blockchain = idle_state.blockchain.write();
            println!("{}: Headers sync: last best tip {}", self.config.network, blockchain.best_tip_hash());

            // Request headers
            consume_err("Headers sync: failed to send `headers` message",
              idle_state.sock.send_message(message::GetHeaders(
                  GetHeadersMessage::new(blockchain.locator_hashes(), zero_hash()))));
            // Loop through received headers
            let mut received_headers = false;
            while !received_headers {
              with_next_message!(idle_state.net_chan.recv(),
                message::Headers(headers) => {
                  for lone_header in headers.iter() {
                    match blockchain.add_header(lone_header.header) {
                      Err(e) => {
                        println!("{}: Headers sync: failed to add {}: {}", self.config.network, lone_header.header.bitcoin_hash(), e);
                      }
                       _ => {}
                    }
                  }
                  received_headers = true;
                  // We are done if this `headers` message did not update our status
                  done = headers.len() == 0;
                }
                message::Ping(nonce) => {
                  consume_err("Warning: failed to send pong in response to ping",
                    idle_state.sock.send_message(message::Pong(nonce)));
                }
              );
            }
          }
          // Done!
          println!("{}: Done sync.", self.config.network);
          SyncUtxoSet(idle_state, Vec::with_capacity(UTXO_SYNC_N_BLOCKS))
        },
        SyncUtxoSet(mut idle_state, mut cache) => {
          let mut failed = false;
          cache.clear();
          // Ugh these scopes are ugly. Can't wait for non-lexically-scoped borrows!
          {
            let blockchain = idle_state.blockchain.read();
            let mut utxo_set = idle_state.utxo_set.write();

            let last_hash = utxo_set.last_hash();
            println!("Starting UTXO sync from {}", last_hash);

            // Unwind any reorg'd blooks
            for block in blockchain.rev_stale_iter(last_hash) {
              println!("Rewinding stale block {}", block.header.bitcoin_hash());
              if !utxo_set.rewind(block) {
                println!("Failed to rewind stale block {}", block.header.bitcoin_hash());
              }
            }
            // Loop through blockchain for new data
            let last_hash = utxo_set.last_hash();
            let mut iter = blockchain.iter(last_hash).enumerate().skip(1).peekable();
            for (count, node) in iter {
              cache.push(Inventory { inv_type: InvBlock, hash: node.block.header.bitcoin_hash() });

              // Every so often, send a new message
              if count % UTXO_SYNC_N_BLOCKS == 0 || iter.is_empty() {
                println!("UTXO sync: n_blocks {} n_utxos {}", count, utxo_set.n_utxos());
                consume_err("UTXO sync: failed to send `getdata` message",
                  idle_state.sock.send_message(message::GetData(cache.clone())));

                let mut block_count = 0;
                let mut recv_data = PatriciaTree::new();
                while block_count < cache.len() {
                  with_next_message!(idle_state.net_chan.recv(),
                    message::Block(block) => {
                      recv_data.insert(&block.header.bitcoin_hash().as_uint128(), 128, block);
                      block_count += 1;
                    }
                    message::NotFound(_) => {
                      println!("UTXO sync: received `notfound` from sync peer, failing sync.");
                      failed = true;
                      block_count += 1;
                    }
                    message::Ping(nonce) => {
                      consume_err("Warning: failed to send pong in response to ping",
                        idle_state.sock.send_message(message::Pong(nonce)));
                    }
                  )
                }
                for recv_inv in cache.iter() {
                  let block_opt = recv_data.lookup(&recv_inv.hash.as_uint128(), 128);
                  match block_opt {
                    Some(block) => {
                      if !utxo_set.update(block) {
                        println!("Failed to update UTXO set with block {}", block.header.bitcoin_hash());
                        failed = true;
                      }
                    }
                    None => {
                      println!("Uh oh, requested block {} but didn't get it!", recv_inv.hash);
                      failed = true;
                    }
                  }
                }
                cache.clear();
              }
            }
          }
          if failed {
            println!("Failed to sync UTXO set, trying to resync chain.");
            SyncBlockchain(idle_state)
          } else {
            // Now that we're done with reorgs, update our cached block data
            let mut hashes_to_drop_data = vec![];
            let mut inv_to_add_data = vec![];
            {
              let blockchain = idle_state.blockchain.read();
              for (n, node) in blockchain.rev_iter(blockchain.best_tip_hash()).enumerate() {
                if n < BLOCKCHAIN_N_FULL_BLOCKS {
                  if !node.has_txdata {
                    inv_to_add_data.push(Inventory { inv_type: InvBlock,
                                                     hash: node.block.header.bitcoin_hash() });
                  }
                } else if node.has_txdata {
                  hashes_to_drop_data.push(node.block.header.bitcoin_hash());
                }
              }
            }
            // Request new block data
            consume_err("UTXO sync: failed to send `getdata` message",
              idle_state.sock.send_message(message::GetData(inv_to_add_data.clone())));
            {
              let mut blockchain = idle_state.blockchain.write();
              // Delete old block data
              for hash in hashes_to_drop_data.move_iter() {
                println!("Dropping old blockdata for {}", hash);
                match blockchain.remove_txdata(hash) {
                  Err(e) => { println!("Failed to remove txdata: {}", e); }
                  _ => {}
                }
              }
              // Receive new block data
              let mut block_count = 0;
              while block_count < inv_to_add_data.len() {
                with_next_message!(idle_state.net_chan.recv(),
                  message::Block(block) => {
                    println!("Adding blockdata for {}", block.header.bitcoin_hash());
                    match blockchain.add_txdata(block) {
                      Err(e) => { println!("Failed to add txdata: {}", e); }
                      _ => {}
                    }
                    block_count += 1;
                  }
                  message::NotFound(_) => {
                    println!("Blockchain sync: received `notfound` on full blockdata, will not be able to handle reorgs past this block.");
                    block_count += 1;
                  }
                  message::Ping(nonce) => {
                    consume_err("Warning: failed to send pong in response to ping",
                    idle_state.sock.send_message(message::Pong(nonce)));
                  }
                )
              }
            }
            SaveToDisk(idle_state)
          }
        },
        // Idle loop
        Idle(mut idle_state) => {
          println!("Idling...");
          let saveout = nu_select!(
            message from idle_state.net_chan => {
              idle_message(&mut idle_state, message);
              false
            },
            () from save_timer => true,
            (request, tx) from self.rpc_rx => {
              tx.send(handle_rpc(request, &mut idle_state));
              false
            }
          );
          if saveout {
            SyncBlockchain(idle_state)
          } else {
            Idle(idle_state)
          }
        },
        // Temporary states
        SaveToDisk(idle_state) => {
          let bc_arc = idle_state.blockchain.clone();
          let us_arc = idle_state.utxo_set.clone();
          let blockchain_path = self.config.blockchain_path.clone();
          let utxo_set_path = self.config.utxo_set_path.clone();
          spawn(proc() {
            // Lock the blockchain for reading while we are saving it.
            {
              let blockchain = bc_arc.read();
              println!("Saving blockchain...");
              match blockchain.serialize_file(&blockchain_path) {
                Ok(()) => { println!("Successfully saved blockchain.") },
                Err(e) => { println!("failed to write blockchain: {:}", e); }
              }
              println!("Done saving blockchain.");
            }
            // Lock the UTXO set for reading while we are saving it.
            {
              let utxo_set = us_arc.read();
              println!("Saving UTXO set...");
              match utxo_set.serialize_file(&utxo_set_path) {
                Ok(()) => { println!("Successfully saved UTXO set.") },
                Err(e) => { println!("failed to write UTXO set: {:}", e); }
              }
              println!("Done saving UTXO set.");
            }
          });
          Idle(idle_state)
        }
      };
    }
  }
}

impl Listener for Bitcoind {
  fn peer<'a>(&'a self) -> &'a str {
    self.config.peer_addr.as_slice()
  }

  fn port(&self) -> u16 {
    self.config.peer_port
  }

  fn network(&self) -> Network {
    self.config.network
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
    message::Addr(_) => {
      println!("Got addr, ignoring since we only support one peer for now.");
    }
    message::Block(block) => {
      let mut lock = idle_state.blockchain.write();
      println!("Received block: {:x}", block.header.bitcoin_hash());
      if lock.get_block(block.header.prev_blockhash).is_some() {
        let mut utxo_lock = idle_state.utxo_set.write();
        // non-orphan, add it
        println!("Received non-orphan, adding to blockchain...");
        if !utxo_lock.update(&block) {
          println!("Failed to update UTXO set with block {}", block.header.bitcoin_hash());
        }
        match lock.add_block(block) {
          Err(e) => {
            println!("Failed to add block: {}", e);
          }
          _ => {}
        }
        println!("Done adding block.");
      } else {
        let lock = lock.downgrade();
        // orphan, send getblocks to get all blocks in order
        println!("Got an orphan, sending a getblocks to get its parents");
        consume_err("Headers sync: failed to send `headers` message",
          idle_state.sock.send_message(message::GetBlocks(
              GetBlocksMessage::new(lock.locator_hashes(),
                                    block.header.prev_blockhash))));
      }
    },
    message::Headers(headers) => {
      for lone_header in headers.iter() {
        println!("Received header: {}, ignoring.", lone_header.header.bitcoin_hash());
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
  use bitcoin::network::constants::BitcoinTestnet;
  use bitcoin::network::listener::Listener;

  use user_data::{blockchain_path, utxo_set_path};
  use bitcoind::Bitcoind;

  #[test]
  fn test_bitcoind() {
    let bitcoind = Bitcoind::new("localhost", 1000,
                                 BitcoinTestnet,
                                 blockchain_path(BitcoinTestnet),
                                 utxo_set_path(BitcoinTestnet));
    assert_eq!(bitcoind.peer(), "localhost");
    assert_eq!(bitcoind.port(), 1000);

    let mut bitcoind = Bitcoind::new("localhost", 0,
                                     BitcoinTestnet,
                                     blockchain_path(BitcoinTestnet),
                                     utxo_set_path(BitcoinTestnet));
    assert!(bitcoind.listen().is_err());
  }
}

