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

//! # The Wizards' Wallet
//!
//! The Wizards' Wallet is a SPV Bitcoin Wallet designed for ease of prototyping
//! and a willingness to experiment with user interfaces, and exposing potentially
//! dangerous or experimental ideas built on top of the Bitcoin protocol.
//!
//! It is also written entirely in Rust to illustrate the benefits of strong type
//! safety, including ownership and lifetime, for financial and/or cryptographic
//! software.
//!


#![crate_id = "wizards-wallet#0.1-pre"]

#![comment = "The Wizards' Wallet Test Root"]
#![license = "CC0"]

// Experimental features we need
#![feature(globs)]

// Coding conventions
#![deny(non_uppercase_pattern_statics)]
#![deny(uppercase_variables)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case_functions)]
#![deny(unused_mut)]
#![warn(missing_doc)]

extern crate rand;
extern crate time;

extern crate bitcoin;

mod bitcoind;

