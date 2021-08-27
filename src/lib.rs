//! `async-zeroconf` is a crate to register ZeroConf services and provides a
//! way of keeping the service alive using a reference to the service which
//! keeps the service registered until it is dropped. Internally, a tokio task
//! is spawned to check for events asynchronously.
//!
//! # Examples
//! ```
//! # tokio_test::block_on(async {
//! // Create a service description
//! let service = async_zeroconf::Service::new("Server", "_http._tcp", 80);
//! // Publish the service
//! let service_ref = service.publish()?;
//! // Service kept alive until service_ref dropped
//! # Ok::<(), async_zeroconf::ZeroconfError>(())
//! # });
//! ```
//!
//! [ServiceBrowserBuilder] and [ServiceResolver] can be used to browse and
//! resolve services respectively.

extern crate bonjour_sys;

mod c_intf;
mod error;
mod interface;
mod service;
mod service_browser;
mod service_ref;
mod service_resolver;
mod txt;

pub(crate) use service_ref::ServiceRefWrapper;

pub use error::{BonjourError, ZeroconfError};
pub use interface::Interface;
pub use service::Service;
pub use service_browser::{ServiceBrowser, ServiceBrowserBuilder};
pub use service_ref::{OpKind, OpType, ProcessTask, ServiceRef};
pub use service_resolver::ServiceResolver;
pub use txt::TxtRecord;

#[cfg(test)]
mod tests;

#[cfg(doctest)]
extern crate doc_comment;

#[cfg(doctest)]
doc_comment::doctest!("../README.md");
