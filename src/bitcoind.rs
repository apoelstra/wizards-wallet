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

use std::collections::{DList, Deque};
use std::default::Default;
use std::io::{File, Open, Write, BufferedReader, BufferedWriter};
use std::io::IoResult;
use std::io::timer::Timer;
use std::sync::{Arc, RWLock};
use std::time::Duration;
use serialize::json;

use jsonrpc;

use bitcoin::blockdata::blockchain::Blockchain;
use bitcoin::blockdata::utxoset::{UtxoSet, TxoValidation, ScriptValidation};
use bitcoin::network::constants::Network;
use bitcoin::network::encodable::{ConsensusEncodable, ConsensusDecodable};
use bitcoin::network::listener::Listener;
use bitcoin::network::socket::Socket;
use bitcoin::network::message::NetworkMessage;
use bitcoin::network::message;
use bitcoin::network::message_blockdata::{GetBlocksMessage, GetHeadersMessage, Inventory, InvBlock};
use bitcoin::network::serialize::{BitcoinHash, RawEncoder, RawDecoder};
use bitcoin::util::patricia_tree::PatriciaTree;
use bitcoin::util::misc::consume_err;

use coinjoin;
use constants::BLOCKCHAIN_N_FULL_BLOCKS;
use constants::UTXO_SYNC_N_BLOCKS;
use constants::SAVE_FREQUENCY;
use rpc_server::handle_rpc;
use user_data::NetworkConfig;

/// Data used by an idling wallet.
pub struct IdleState {
  sock: Socket,
  net_chan: Receiver<NetworkMessage>,
  /// Network that we're on
  pub config: NetworkConfig,
  /// Coinjoin server
  pub coinjoin: Option<coinjoin::server::Server>,
  /// Mutex for blockchain access
  pub blockchain: Arc<RWLock<Blockchain>>,
  /// Mutex for UTXO set access
  pub utxo_set: Arc<RWLock<UtxoSet>>
}

enum WalletAction {
  SyncBlockchain,
  SyncUtxoSet,
  SaveToDisk,
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
    let save_timer = timer.periodic(Duration::seconds(SAVE_FREQUENCY));
    let mut state_queue = DList::new();

    // Startup
    // Open socket
    let (chan, sock) = try!(self.start());
    // Load cached blockchain and UTXO set from disk
    println!("{}: Loading blockchain...", self.config.network);
    // Load blockchain from disk
    let mut decoder = RawDecoder::new(BufferedReader::new(File::open(&self.config.blockchain_path)));
    let blockchain = match ConsensusDecodable::consensus_decode(&mut decoder) {
      Ok(blockchain) => blockchain,
      Err(e) => {
        println!("{}: Failed to load blockchain: {:}, starting from genesis.", self.config.network, e);
        Blockchain::new(self.config.network)
      }
    };
    println!("{}: Loading utxo set...", self.config.network);
    // Load UTXO set from disk
    let mut decoder = RawDecoder::new(BufferedReader::new(File::open(&self.config.utxo_set_path)));
    let utxo_set = match ConsensusDecodable::consensus_decode(&mut decoder) {
      Ok(utxo_set) => utxo_set,
      Err(e) => {
        println!("{}: Failed to load UTXO set: {:}, starting from genesis.", self.config.network, e);
        UtxoSet::new(self.config.network, BLOCKCHAIN_N_FULL_BLOCKS)
      }
    };
    // Setup idle state
    let mut idle_state = IdleState {
      sock: sock,
      net_chan: chan,
      // TODO: I'd rather this clone be some sort of take, but we need `self.config`
      //       to be around for the `Listener` trait getters below. Rework this.
      config: self.config.clone(),
      blockchain: Arc::new(RWLock::new(blockchain)),
      utxo_set: Arc::new(RWLock::new(utxo_set)),
      coinjoin: None
    };

    // Eternal state machine loop
    state_queue.push(SyncBlockchain);
    state_queue.push(SyncUtxoSet);
    state_queue.push(SaveToDisk);
    loop {
      match state_queue.pop_front() {
        // Synchronize the blockchain with the peer
        Some(SyncBlockchain) => {
          // Do a headers-first sync of all blocks
          let mut done = false;
          while !done {
            // Borrow the blockchain mutably
            let mut blockchain = idle_state.blockchain.write();
            println!("{}: Headers sync: last best tip {}",
                     idle_state.config.network, blockchain.best_tip_hash());

            // Request headers
            consume_err("Headers sync: failed to send `headers` message",
              idle_state.sock.send_message(message::GetHeaders(
                  GetHeadersMessage::new(blockchain.locator_hashes(), Default::default()))));
            // Loop through received headers
            let mut received_headers = false;
            while !received_headers {
              with_next_message!(idle_state.net_chan.recv(),
                message::Headers(headers) => {
                  for lone_header in headers.iter() {
                    match blockchain.add_header(lone_header.header) {
                      Err(e) => {
                        println!("{}: Headers sync: failed to add {}: {}", 
                                 idle_state.config.network, lone_header.header.bitcoin_hash(), e);
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
          println!("{}: Done sync.", idle_state.config.network);
        },
        Some(SyncUtxoSet) => {
          let mut failed = false;
          let mut cache = Vec::with_capacity(UTXO_SYNC_N_BLOCKS);
          // Ugh these scopes are ugly. Can't wait for non-lexically-scoped borrows!
          {
            let blockchain = idle_state.blockchain.read();
            let last_hash = {
              let mut utxo_set = idle_state.utxo_set.write();
              let last_hash = utxo_set.last_hash();
              println!("{}: Starting UTXO sync from {}", idle_state.config.network, last_hash);

              // Unwind any reorg'd blooks
              for block in blockchain.rev_stale_iter(last_hash) {
                println!("{}: Rewinding stale block {}",
                         idle_state.config.network, block.bitcoin_hash());
                if !utxo_set.rewind(block) {
                  println!("{}: Failed to rewind stale block {}",
                           idle_state.config.network, block.bitcoin_hash());
                }
              }
              utxo_set.last_hash()
            };
            // Loop through blockchain for new data
            let mut iter = blockchain.iter(last_hash).enumerate().skip(1).peekable();
            for (count, node) in iter {
              // Reborrow blockchain and utxoset on each iter
              cache.push(Inventory { inv_type: InvBlock, hash: node.block.bitcoin_hash() });

              // Every so often, send a new message
              if count % UTXO_SYNC_N_BLOCKS == 0 || iter.is_empty() {
                let mut utxo_set = idle_state.utxo_set.write();
                println!("{}: UTXO sync: n_blocks {} n_utxos {} pruned {}",
                         idle_state.config.network, count, utxo_set.n_utxos(), utxo_set.n_pruned());
                consume_err("UTXO sync: failed to send `getdata` message",
                  idle_state.sock.send_message(message::GetData(cache.clone())));

                let mut block_count = 0;
                let mut recv_data = PatriciaTree::new();
                while block_count < cache.len() {
                  with_next_message!(idle_state.net_chan.recv(),
                    message::Block(block) => {
                      recv_data.insert(&block.bitcoin_hash().into_uint128(), 128, block);
                      block_count += 1;
                    }
                    message::NotFound(_) => {
                      println!("{}: UTXO sync: received `notfound` from sync peer, failing sync.",
                               idle_state.config.network);
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
                  let block_opt = recv_data.lookup(&recv_inv.hash.into_uint128(), 128);
                  match block_opt {
                    Some(block) => {
                      match utxo_set.update(block, TxoValidation) {
                        Ok(_) => {}
                        Err(e) => {
                          println!("{}: Failed to update UTXO set with block {}: {}",
                                   idle_state.config.network, block.bitcoin_hash(), e);
                          failed = true;
                        }
                      }
                    }
                    None => {
                      println!("{}: Uh oh, requested block {} but didn't get it!",
                               idle_state.config.network, recv_inv.hash);
                      failed = true;
                    }
                  }
                }
                cache.clear();
              }
            }
          }
          if failed {
            println!("{}: Failed to sync UTXO set, will resync chain and try again.",
                     idle_state.config.network);
            state_queue.push(SyncBlockchain);
            state_queue.push(SyncUtxoSet);
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
                                                     hash: node.block.bitcoin_hash() });
                  }
                } else if node.has_txdata {
                  hashes_to_drop_data.push(node.block.bitcoin_hash());
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
                println!("{}, Dropping old blockdata for {}", idle_state.config.network, hash);
                match blockchain.remove_txdata(hash) {
                  Err(e) => { println!("{}: Failed to remove txdata: {}",
                                       idle_state.config.network, e); }
                  _ => {}
                }
              }
              // Receive new block data
              let mut block_count = 0;
              while block_count < inv_to_add_data.len() {
                with_next_message!(idle_state.net_chan.recv(),
                  message::Block(block) => {
                    println!("{}: Adding blockdata for {}",
                             idle_state.config.network, block.bitcoin_hash());
                    match blockchain.add_txdata(block) {
                      Err(e) => { println!("{}: Failed to add txdata: {}",
                                           idle_state.config.network, e); }
                      _ => {}
                    }
                    block_count += 1;
                  }
                  message::NotFound(_) => {
                    println!("{}: Blockchain sync: received `notfound` on full blockdata, \
                              will not be able to handle reorgs past this block.",
                             idle_state.config.network);
                    block_count += 1;
                  }
                  message::Ping(nonce) => {
                    consume_err("Warning: failed to send pong in response to ping",
                    idle_state.sock.send_message(message::Pong(nonce)));
                  }
                )
              }
            }
          }
        },
        // Idle loop
        None => {
          println!("{}: Idling...", idle_state.config.network);
          nu_select!(
            message from idle_state.net_chan => {
              idle_message(&mut idle_state, message);
            },
            () from save_timer => {
              state_queue.push(SyncBlockchain);
              state_queue.push(SyncUtxoSet);
              state_queue.push(SaveToDisk);
            },
            (request, tx) from self.rpc_rx => {
              tx.send(handle_rpc(request, &mut idle_state));
            }
          );
        },
        // Temporary states
        Some(SaveToDisk) => {
          let bc_arc = idle_state.blockchain.clone();
          let us_arc = idle_state.utxo_set.clone();
          let blockchain_path = idle_state.config.blockchain_path.clone();
          let utxo_set_path = idle_state.config.utxo_set_path.clone();
          let network = idle_state.config.network;
          spawn(proc() {
            // Lock the blockchain for reading while we are saving it.
            {
              let blockchain = bc_arc.read();
              println!("{}: Saving blockchain...", network);
              let mut encoder = RawEncoder::new(BufferedWriter::new(File::open_mode(&blockchain_path, Open, Write)));
              match blockchain.consensus_encode(&mut encoder) {
                Ok(()) => { println!("{}: Successfully saved blockchain.", network) },
                Err(e) => { println!("{}: Failed to write blockchain: {:}", network, e); }
              }
            }
            // Lock the UTXO set for reading while we are saving it.
            {
              let utxo_set = us_arc.read();
              println!("{}: Saving UTXO set...", network);
              let mut encoder = RawEncoder::new(BufferedWriter::new(File::open_mode(&utxo_set_path, Open, Write)));
              match utxo_set.consensus_encode(&mut encoder) {
                Ok(()) => { println!("{}: Successfully saved UTXO set.", network) },
                Err(e) => { println!("{}: Failed to write UTXO set: {:}", network, e); }
              }
            }
          });
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
      consume_err("Warning: failed to send verack in response to version",
        idle_state.sock.send_message(message::Verack));
    }
    message::Verack => {}
    message::Addr(_) => {
      // Ignore addr until we get multipeer support
    }
    message::Block(block) => {
      let mut lock = idle_state.blockchain.write();
      println!("{}, Received block: {:x}", idle_state.config.network, block.bitcoin_hash());
      if lock.get_block(block.header.prev_blockhash).is_some() {
        let mut utxo_lock = idle_state.utxo_set.write();
        // non-orphan, add it
        println!("{}, Received non-orphan, adding to blockchain...", idle_state.config.network);
        match utxo_lock.update(&block, ScriptValidation) {
          Ok(_) => {}
          Err(e) => {
            println!("{}, Failed to update UTXO set with block {}: {}",
                     idle_state.config.network, block.bitcoin_hash(), e);
          }
        }
        match lock.add_block(block) {
          Err(e) => {
            println!("{}, Failed to add block: {}", idle_state.config.network, e);
          }
          _ => {}
        }
        println!("{}, Done adding block.", idle_state.config.network);
      } else {
        let lock = lock.downgrade();
        // orphan, send getblocks to get all blocks in order
        println!("{}, Got an orphan, sending a getblocks to get its parents", idle_state.config.network);
        consume_err("Headers sync: failed to send `headers` message",
          idle_state.sock.send_message(message::GetBlocks(
              GetBlocksMessage::new(lock.locator_hashes(),
                                    block.header.prev_blockhash))));
      }
    },
    message::Headers(headers) => {
      for lone_header in headers.iter() {
        println!("{}, Received header: {}, ignoring.",
                 idle_state.config.network, lone_header.header.bitcoin_hash());
      }
    },
    message::Inv(inv) => {
      println!("{}, Received inv.", idle_state.config.network);
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

