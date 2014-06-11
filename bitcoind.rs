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

use bitcoin::network::listener::Listener;

pub struct Bitcoind {
  peer_address: String,
  peer_port: u16
}

impl Bitcoind {
  pub fn new(peer_address: &str, peer_port: u16) -> Bitcoind {
    Bitcoind {
      peer_address: String::from_str(peer_address),
      peer_port: peer_port
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



#[test]
fn test_bitcoind() {
  let bitcoind = Bitcoind::new("localhost", 1000);
  assert_eq!(bitcoind.peer(), "localhost");
  assert_eq!(bitcoind.port(), 1000);
}


