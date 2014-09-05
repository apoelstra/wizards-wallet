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
use time;

use jsonrpc;

use bitcoin::blockdata::blockchain::Blockchain;
use bitcoin::blockdata::utxoset::{UtxoSet, ValidationLevel, TxoValidation, ScriptValidation};
use bitcoin::network::constants::Network;
use bitcoin::network::encodable::{ConsensusEncodable, ConsensusDecodable};
use bitcoin::network::listener::Listener;
use bitcoin::network::socket::Socket;
use bitcoin::network::message::NetworkMessage;
use bitcoin::network::message;
use bitcoin::network::message_blockdata::{GetHeadersMessage, Inventory, InvBlock};
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
  net_chan: Receiver<NetworkMessage>,
  /// Socket used to send network messages
  pub sock: Socket,
  /// Network that we're on
  pub config: NetworkConfig,
  /// Lowest debug level which we output messages for
  debug_level: DebugLevel,
  /// Coinjoin server
  pub coinjoin: Option<coinjoin::server::Server>,
  /// Mutex for blockchain access
  pub blockchain: Arc<RWLock<Blockchain>>,
  /// Mutex for UTXO set access
  pub utxo_set: Arc<RWLock<UtxoSet>>
}

enum WalletAction {
  SyncBlockchain,
  SyncUtxoSet(ValidationLevel),
  SaveToDisk,
}

#[deriving(Clone, PartialEq, Eq, PartialOrd, Ord, Show)]
enum DebugLevel {
  Debug,
  Notice,
  Status,
  Warning,
  Error,
  Fatal
}

/// The main Bitcoin network listener structure
pub struct Bitcoind {
  /// Configuration for this network
  config: NetworkConfig,
  /// Lowest debug level which we output messages for
  debug_level: DebugLevel,
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

macro_rules! debug(
  (($network:expr, $debug_level:expr), $level:ident, $fmt:expr $(, $arg:expr)*) => (
    if $level >= $debug_level {
      println!(concat!("{} [{}] {}: ", $fmt),
               time::now().rfc3339(),
               $level, $network,
               $($arg),*);
    }
  );
  ($bitcoind:ident, $level:ident, $fmt:expr $(, $arg:expr)*) => (
    if $level >= $bitcoind.debug_level {
      println!(concat!("{} [{}] {}: ", $fmt),
               time::now().rfc3339(),
               $level, $bitcoind.config.network,
               $($arg),*);
    }
  );
)

impl Bitcoind {
  /// Constructor
  pub fn new(config: NetworkConfig,
             rpc_rx: Receiver<(jsonrpc::Request, Sender<jsonrpc::JsonResult<json::Json>>)>)
             -> Bitcoind {
    Bitcoind {
      config: config,
      rpc_rx: rpc_rx,
      debug_level: Notice
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
    debug!(self, Status, "Loading blockchain...");
    // Load blockchain from disk
    let mut decoder = RawDecoder::new(BufferedReader::new(File::open(&self.config.blockchain_path)));
    let blockchain = match ConsensusDecodable::consensus_decode(&mut decoder) {
      Ok(blockchain) => blockchain,
      Err(e) => {
        debug!(self, Error, "Failed to load blockchain: {:}, starting from genesis.", e);
        Blockchain::new(self.config.network)
      }
    };
    debug!(self, Status, "Loading utxo set...");
    // Load UTXO set from disk
    let mut decoder = RawDecoder::new(BufferedReader::new(File::open(&self.config.utxo_set_path)));
    let utxo_set = match ConsensusDecodable::consensus_decode(&mut decoder) {
      Ok(utxo_set) => utxo_set,
      Err(e) => {
        debug!(self, Error, "Failed to load UTXO set: {:}, starting from genesis.", e);
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
      debug_level: self.debug_level,
      blockchain: Arc::new(RWLock::new(blockchain)),
      utxo_set: Arc::new(RWLock::new(utxo_set)),
      coinjoin: None
    };

    // Eternal state machine loop
    state_queue.push(SyncBlockchain);
    state_queue.push(SyncUtxoSet(TxoValidation));  // for initial sync only do TXO validation
    state_queue.push(SaveToDisk);
    loop {
      match state_queue.pop_front() {
        // Synchronize the blockchain with the peer
        Some(SyncBlockchain) => {
          debug!(idle_state, Status, "Syncing blockheaders: last best tip {:x}",
                 blockchain.best_tip_hash());
          // Do a headers-first sync of all blocks
          let mut done = false;
          while !done {
            // Borrow the blockchain mutably
            let mut blockchain = idle_state.blockchain.write();
            debug!(idle_state, Notice, "Starting headers sync from {:x}",
                   blockchain.best_tip_hash());

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
                        debug!(idle_state, Error, "Headers sync: failed to add {:x}: {}", 
                               lone_header.header.bitcoin_hash(), e);
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
          debug!(idle_state, Status, "Done headers sync.");
        },
        Some(SyncUtxoSet(validation_level)) => {
          let mut failed = false;
          let mut cache = Vec::with_capacity(UTXO_SYNC_N_BLOCKS);
          // Ugh these scopes are ugly. Can't wait for non-lexically-scoped borrows!
          {
            let blockchain = idle_state.blockchain.read();
            let last_hash = {
              let mut utxo_set = idle_state.utxo_set.write();
              let last_hash = utxo_set.last_hash();
              debug!(idle_state, Status, "Starting UTXO sync from {:x}", last_hash);

              // Unwind any reorg'd blooks
              for block in blockchain.rev_stale_iter(last_hash) {
                debug!(idle_state, Notice, "Rewinding stale block {}", block.bitcoin_hash());
                if !utxo_set.rewind(block) {
                  debug!(idle_state, Notice, " Failed to rewind stale block {}",
                         block.bitcoin_hash());
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
                debug!(idle_state, Notice, "UTXO sync: n_blocks {} n_utxos {} pruned {}",
                       count, utxo_set.n_utxos(), utxo_set.n_pruned());
                consume_err("UTXO sync: failed to send `getdata` message",
                  idle_state.sock.send_message(message::GetData(cache.clone())));

                let mut block_count = 0;
                let mut recv_data = PatriciaTree::new();
                while block_count < cache.len() {
                  with_next_message!(idle_state.net_chan.recv(),
                    message::Block(block) => {
                      recv_data.insert(&block.bitcoin_hash().into_le().low_128(), 128, block);
                      block_count += 1;
                    }
                    message::NotFound(_) => {
                      debug!(idle_state, Error,
                             "UTXO sync: received `notfound` from sync peer, failing sync.");
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
                  let block_opt = recv_data.lookup(&recv_inv.hash.into_le().low_128(), 128);
                  match block_opt {
                    Some(block) => {
                      match utxo_set.update(block, validation_level) {
                        Ok(_) => {}
                        Err(e) => {
                          debug!(idle_state, Error,
                                 "Failed to update UTXO set with block {:x}: {}",
                                 block.bitcoin_hash(), e);
                          failed = true;
                        }
                      }
                    }
                    None => {
                      debug!(idle_state, Error, "Uh oh, requested block {:x} but didn't get it!",
                             recv_inv.hash);
                      failed = true;
                    }
                  }
                }
                cache.clear();
              }
            }
          }
          if failed {
            debug!(idle_state, Error, "Failed to sync UTXO set, will resync chain and try again.");
            state_queue.push(SyncBlockchain);
            state_queue.push(SyncUtxoSet(validation_level));
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
                debug!(idle_state, Notice, "Dropping old blockdata for {:x}", hash);
                match blockchain.remove_txdata(hash) {
                  Err(e) => { debug!(idle_state, Error, "Failed to remove txdata: {}", e); }
                  _ => {}
                }
              }
              // Receive new block data
              let mut block_count = 0;
              while block_count < inv_to_add_data.len() {
                with_next_message!(idle_state.net_chan.recv(),
                  message::Block(block) => {
                    debug!(idle_state, Notice, "Adding blockdata for {:x}", block.bitcoin_hash());
                    match blockchain.add_txdata(block) {
                      Err(e) => { debug!(idle_state, Error, "Failed to add txdata: {}", e); }
                      _ => {}
                    }
                    block_count += 1;
                  }
                  message::NotFound(_) => {
                    debug!(idle_state, Error,
                           "Blockchain sync: received `notfound` on full blockdata, \
                           will not be able to handle reorgs past this block.");
                    block_count += 1;
                  }
                  message::Ping(nonce) => {
                    consume_err("Warning: failed to send pong in response to ping",
                    idle_state.sock.send_message(message::Pong(nonce)));
                  }
                )
              }
            }
            debug!(idle_state, Status, "Done UTXO sync.");
          }
        },
        // Idle loop
        None => {
          debug!(idle_state, Debug, "Idling...");
          nu_select!(
            message from idle_state.net_chan => {
              idle_message(&mut state_queue, &mut idle_state, message);
            },
            () from save_timer => {
              state_queue.push(SyncBlockchain);
              state_queue.push(SyncUtxoSet(ScriptValidation));
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
          let debug_level = idle_state.debug_level;
          spawn(proc() {
            // Lock the blockchain for reading while we are saving it.
            {
              let blockchain = bc_arc.read();
              debug!((network, debug_level), Status, "Saving blockchain...");
              let mut encoder = RawEncoder::new(BufferedWriter::new(File::open_mode(&blockchain_path, Open, Write)));
              match blockchain.consensus_encode(&mut encoder) {
                Ok(()) => { debug!((network, debug_level), Status,
                                   "Done saving blockchain."); },
                Err(e) => { debug!((network, debug_level), Error,
                            "Failed to write blockchain: {}", e); }
              }
            }
            // Lock the UTXO set for reading while we are saving it.
            {
              let utxo_set = us_arc.read();
              debug!((network, debug_level), Status, "Saving UTXO set...");
              let mut encoder = RawEncoder::new(BufferedWriter::new(File::open_mode(&utxo_set_path, Open, Write)));
              match utxo_set.consensus_encode(&mut encoder) {
                Ok(()) => { debug!((network, debug_level), Status,
                                   "Done saving UTXO set.") },
                Err(e) => { debug!((network, debug_level), Error,
                                   "Failed to write UTXO set: {:}", e); }
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
fn idle_message<S:Deque<WalletAction>>(state_queue: &mut S,
                                       idle_state: &mut IdleState,
                                       message: NetworkMessage) {
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
      debug!(idle_state, Notice, "Received block: {:x}", block.bitcoin_hash());
      if lock.get_block(block.header.prev_blockhash).is_some() {
        // non-orphan, add it
        debug!(idle_state, Notice, "Received non-orphan, adding to blockchain...");
        match lock.add_block(block) {
          Err(e) => {
            debug!(idle_state, Error, "Failed to add block: {}", e);
          }
          _ => {}
        }
        debug!(idle_state, Notice, "Done adding block.");
      } else {
        debug!(idle_state, Notice, "Received orphan, resyncing blockchain...");
        state_queue.push(SyncBlockchain);
      }
      // In either case we want to sync the UTXO set afterward
      state_queue.push(SyncUtxoSet(ScriptValidation));
    },
    message::Headers(headers) => {
      for lone_header in headers.iter() {
        debug!(idle_state, Debug, "Received header: {:x}, ignoring.",
               lone_header.header.bitcoin_hash());
      }
    },
    message::Inv(inv) => {
      debug!(idle_state, Debug, "Received inv.");
      let sendmsg = message::GetData(inv);
      // Send
      consume_err("Warning: failed to send getdata in response to inv",
        idle_state.sock.send_message(sendmsg));
    }
    message::Tx(_) => {
      debug!(idle_state, Debug, "Received tx, ignoring");
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
  // TODO
}

