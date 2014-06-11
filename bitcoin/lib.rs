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

#![crate_id = "bitcoin#0.1-pre"]
#![crate_type = "dylib"]
#![crate_type = "rlib"]

#![feature(macro_rules)]
#![feature(log_syntax)]
#![feature(trace_macros)]

#![comment = "Rust Bitcoin Library"]
#![license = "CC0"]

#![deny(non_camel_case_types)]

extern crate time;
extern crate rand;
extern crate serialize;

extern crate crypto = "rust-crypto";

pub mod network;
pub mod blockdata;
pub mod util;

