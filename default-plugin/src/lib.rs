//! Built-in LogMaker plugin.
//!
//! Makers: `Date`, `IP`, `IPRange`, `NumberRange`, `Pick`, `Regex`, `UUID`.
//! Senders: `Debug`, `Syslog`, `Kafka`.

mod java_date;
mod makers;
mod regex_gen;
mod senders;

use logmaker_plugin_api::{PluginDefinition, PluginInfo};

/// Plugin id of the built-in plugin (same as the Java `default-plugin` JAR).
pub const PLUGIN_ID: &str = "default-plugin";

pub fn definition() -> PluginDefinition {
    PluginDefinition {
        info: PluginInfo {
            id: PLUGIN_ID.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            provider: "m8928".to_owned(),
        },
        makers: vec![
            Box::new(makers::RegexFactory),
            Box::new(makers::PickFactory),
            Box::new(makers::NumberRangeFactory),
            Box::new(makers::DateFactory),
            Box::new(makers::UuidFactory),
            Box::new(makers::IpFactory),
            Box::new(makers::IpRangeFactory),
        ],
        senders: vec![
            Box::new(senders::SyslogFactory),
            Box::new(senders::KafkaFactory),
            Box::new(senders::DebugFactory),
        ],
    }
}
