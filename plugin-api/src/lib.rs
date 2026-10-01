//! Plugin contracts for LogMaker.
//!
//! A plugin contributes **Maker** types (value generators such as IP, Date or
//! Regex) and **Sender** types (delivery targets such as Syslog or Kafka).
//!
//! * Built-in plugins implement [`MakerFactory`] / [`SenderFactory`] directly and
//!   are linked into the server.
//! * External plugins are native libraries (`cdylib`) that build a
//!   [`PluginDefinition`] and expose it with [`export_plugin!`]; the server loads
//!   them at runtime through the stable C ABI in [`ffi`].
//!
//! The host validates user arguments against [`MakerFactory::args`] /
//! [`SenderFactory::args`] (see [`check_args`]) before calling `create`, so
//! factories can read arguments with the `arg_*` helpers without re-validating
//! their types.

mod args;
pub mod ffi;

use std::fmt;

pub use args::{ArgSpec, ArgType, Args, arg_bool, arg_i64, arg_str, arg_string_list, check_args};

/// Error returned by plugin factories and senders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// An argument is missing or has an unusable value. Carries the argument
    /// name when it is known.
    InvalidArgument(Option<String>),
    /// Any other failure, with a human-readable message.
    Failed(String),
}

impl PluginError {
    pub fn invalid(name: impl Into<String>) -> Self {
        Self::InvalidArgument(Some(name.into()))
    }

    pub fn failed(message: impl fmt::Display) -> Self {
        Self::Failed(message.to_string())
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument(Some(name)) => write!(f, "invalid argument: {name}"),
            Self::InvalidArgument(None) => f.write_str("invalid argument"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for PluginError {}

/// A configured value generator. Instances are shared between runtime threads,
/// so `get_data` takes `&self`; use interior mutability for state.
///
/// Resources are released in `Drop`.
pub trait Maker: Send + Sync {
    /// Produces the next value.
    fn get_data(&self) -> String;

    /// Number of pre-generated values buffered by the maker (0 when values are
    /// generated on demand).
    fn size(&self) -> u64 {
        0
    }
}

/// A configured delivery target. Instances are shared between runtime threads.
///
/// Resources (sockets, producers, background threads) are released in `Drop`.
pub trait Sender: Send + Sync {
    /// Delivers one generated log line.
    fn send(&self, data: &str) -> Result<(), PluginError>;
}

/// Creates [`Maker`] instances of one type.
pub trait MakerFactory: Send + Sync {
    /// Type name shown in the UI and stored in maker definitions (e.g. `"IP"`).
    fn type_name(&self) -> &str;

    /// Arguments accepted by this type, in display order.
    fn args(&self) -> Vec<ArgSpec>;

    /// Creates a maker. `args` has already passed [`check_args`] against
    /// [`MakerFactory::args`].
    fn create(&self, name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError>;
}

/// Creates [`Sender`] instances of one type.
pub trait SenderFactory: Send + Sync {
    /// Type name shown in the UI and stored in sender definitions (e.g. `"Syslog"`).
    fn type_name(&self) -> &str;

    /// Arguments accepted by this type, in display order.
    fn args(&self) -> Vec<ArgSpec>;

    /// Creates a sender. `args` has already passed [`check_args`] against
    /// [`SenderFactory::args`].
    fn create(&self, name: &str, args: &Args) -> Result<Box<dyn Sender>, PluginError>;
}

/// Plugin identity shown on the plugin page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub version: String,
    pub provider: String,
}

/// Everything a plugin contributes.
pub struct PluginDefinition {
    pub info: PluginInfo,
    pub makers: Vec<Box<dyn MakerFactory>>,
    pub senders: Vec<Box<dyn SenderFactory>>,
}
