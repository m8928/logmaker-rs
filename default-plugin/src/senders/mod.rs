//! Built-in sender types.

mod kafka;
mod syslog;

use logmaker_plugin_api::{ArgSpec, Args, PluginError, Sender, SenderFactory};

pub use kafka::KafkaFactory;
pub use syslog::SyslogFactory;

/// Writes every line to the server log.
pub struct DebugFactory;

struct DebugSender {
    name: String,
}

impl SenderFactory for DebugFactory {
    fn type_name(&self) -> &str {
        "Debug"
    }

    fn args(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn create(&self, name: &str, _args: &Args) -> Result<Box<dyn Sender>, PluginError> {
        Ok(Box::new(DebugSender { name: name.to_owned() }))
    }
}

impl Sender for DebugSender {
    fn send(&self, data: &str) -> Result<(), PluginError> {
        tracing::info!(target: "logmaker::debug_sender", sender = %self.name, "{data}");
        Ok(())
    }
}
