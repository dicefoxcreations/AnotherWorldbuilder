//! Worlds between devices. On the local network: found over mDNS, swapped
//! over QUIC — no public relay, no address lookup. Further away: met
//! through a shared folder that something else keeps in step, then directly.
pub mod admission;
pub mod world;
