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

#![crate_id = "wizards-wallet#0.1-pre"]

#![comment = "The Wizards' Wallet"]
#![license = "CC0"]

#![deny(non_camel_case_types)]

extern crate rand;
extern crate time;

extern crate bitcoin;

#[cfg(not(test))]
use bitcoin::network::listener::Listener;
#[cfg(not(test))]
use bitcoind::Bitcoind;

mod bitcoind;

/// Entry point
#[cfg(not(test))]
fn main()
{
  println!("Starting the Wizards' Wallet");

  // Connect to bitcoind
  let bitcoind = Bitcoind::new("127.0.0.1", 8333);
  loop {
    match bitcoind.start() {
      Err(e) => {
        println!("Got error {:}, trying to connect again...", e);
      }
      _ => {}
    }
  }
}




