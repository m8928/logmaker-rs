//! Example external plugin.
//!
//! * `Counter` maker: `<prefix><n>` with `n` counting up from `start`.
//! * `File` sender: appends each line to the file at `path`.
//!
//! Build with `cargo build --release -p logmaker-sample-plugin` and upload
//! `target/release/liblogmaker_sample_plugin.{so,dylib}` (or the `.dll`) on the
//! Plugin page.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use logmaker_plugin_api::{
    ArgSpec, ArgType, Args, Maker, MakerFactory, PluginDefinition, PluginError, PluginInfo, Sender, SenderFactory,
    arg_i64, arg_str, export_plugin,
};

struct CounterFactory;

struct Counter {
    prefix: String,
    next: AtomicI64,
}

impl MakerFactory for CounterFactory {
    fn type_name(&self) -> &str {
        "Counter"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::optional("prefix", ArgType::String, "Text placed before the number."),
            ArgSpec::optional("start", ArgType::Number, "First number (default 1)."),
        ]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        Ok(Box::new(Counter {
            prefix: arg_str(args, "prefix").unwrap_or_default().to_owned(),
            next: AtomicI64::new(arg_i64(args, "start").transpose()?.unwrap_or(1)),
        }))
    }
}

impl Maker for Counter {
    fn get_data(&self) -> String {
        format!("{}{}", self.prefix, self.next.fetch_add(1, Ordering::Relaxed))
    }
}

struct FileFactory;

struct FileSender {
    out: Mutex<BufWriter<File>>,
}

impl SenderFactory for FileFactory {
    fn type_name(&self) -> &str {
        "File"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::required(
            "path",
            ArgType::String,
            "File the lines are appended to.",
        )]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Sender>, PluginError> {
        let path = arg_str(args, "path").unwrap_or_default();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|_| PluginError::invalid("path"))?;
        Ok(Box::new(FileSender {
            out: Mutex::new(BufWriter::new(file)),
        }))
    }
}

impl Sender for FileSender {
    fn send(&self, data: &str) -> Result<(), PluginError> {
        let mut out = self
            .out
            .lock()
            .map_err(|_| PluginError::failed("file writer poisoned"))?;
        writeln!(out, "{data}")
            .and_then(|()| out.flush())
            .map_err(PluginError::failed)
    }
}

fn definition() -> PluginDefinition {
    PluginDefinition {
        info: PluginInfo {
            id: "sample-plugin".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            provider: "LogMaker examples".into(),
        },
        makers: vec![Box::new(CounterFactory)],
        senders: vec![Box::new(FileFactory)],
    }
}

export_plugin!(definition);
