//! A world on disk: one SQLite database of plain rows, its pictures beside
//! it, and the files written durably.
pub mod db;
pub mod diff;
pub mod durable;
pub mod media;
