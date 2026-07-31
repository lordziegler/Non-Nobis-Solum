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

pub mod core;
pub mod infra;
