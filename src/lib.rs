//! Library root: both front-ends (`non_nobis_solum` CLI and `nns-tui`)
//! are thin binaries over these modules, so the core and its adapters are
//! compiled once and shared.
//!
//! Non Nobis Solum — fertilization planning from soil analysis.
//! Copyright (C) 2026 Sebastian Ziegler
//!
//! This program is free software: you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the
//! Free Software Foundation, either version 3 of the License, or (at your
//! option) any later version.
//!
//! This program is distributed in the hope that it will be useful, but
//! WITHOUT ANY WARRANTY; without even the implied warranty of
//! MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
//! General Public License for more details.
//!
//! You should have received a copy of the GNU General Public License along
//! with this program. If not, see <https://www.gnu.org/licenses/>.

// Deliberate `clippy::pedantic` suppressions. Each one is a lint whose
// prescribed fix makes this crate worse, not a lint left unread; every
// other pedantic warning is fixed at the site.
//
// The numeric-cast family fires on every `f64 -> integer` in the crate, and
// in all of them the truncation *is* the intent, with the bound established
// one expression earlier: sort keys (`(kg * 1000.0).round() as i64`), bag
// counts (`.ceil().max(0.0) as u64`), page and terminal geometry (a cell is
// indivisible), and `len() as f64` for an average. None carries a value that
// can approach the target type's range. Left crate-wide rather than as ~50
// scattered attributes; if a cast over an unbounded quantity is ever added,
// this block is what hid it, so re-audit here before trusting it.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
// `write!` would replace an infallible `push_str` with a discarded
// `fmt::Result`. Swallowing an error is worse signal than one allocation on
// a path that runs once per report.
#![allow(clippy::format_push_string)]
// Every hit is an `assert_eq!` on a value the test computes exactly (a
// conversion factor, a worked example from the source tables). Epsilon
// comparisons would loosen assertions that are correctly pinning a bit
// pattern. Scoped to test builds so production comparisons stay linted.
#![cfg_attr(test, allow(clippy::float_cmp))]

pub mod core;
pub mod infra;
