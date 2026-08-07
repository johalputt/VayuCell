// SPDX-License-Identifier: Apache-2.0

//! VayuCell core.
//!
//! Turns a retired phone into a server whose capabilities are enumerated rather
//! than assumed, whose battery risk is actively governed rather than ignored,
//! and which never reports a capability it has not verified on the device
//! actually in front of it.
//!
//! See `CHARTER.md` for the constraints this code may not violate — in
//! particular Article III, which puts safety of persons ahead of every
//! schedule, and Article IV, which defines what an indicator is allowed to
//! mean.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]

pub mod battery;
pub mod capability;
pub mod csp;
pub mod durability;
pub mod governor;
pub mod headers;
pub mod host;
pub mod ingress;
pub mod panel;
pub mod runtime;
pub mod sampler;
pub mod shed;
pub mod sysfs;
pub mod tier;

#[cfg(test)]
mod capability_test;
#[cfg(test)]
mod csp_test;
#[cfg(test)]
mod durability_test;
#[cfg(test)]
mod governor_test;
#[cfg(test)]
mod headers_test;
#[cfg(test)]
mod ingress_test;
#[cfg(test)]
mod panel_test;
#[cfg(test)]
mod runtime_test;
#[cfg(test)]
mod sampler_test;
#[cfg(test)]
mod shed_test;
#[cfg(test)]
mod sysfs_test;
#[cfg(test)]
mod tier_test;
