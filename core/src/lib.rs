//! LogMaker server: plugin registry, maker/sender registries, log and scenario
//! runtimes, and the REST API that serves the web UI.

mod api_result;
mod dashboard;
mod log;
mod maker;
mod plugin;
mod refs;
mod routes;
mod scenario;
mod sender;
mod storage;
mod template;
mod worker;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use logmaker_plugin_api::PluginDefinition;
use serde::Serialize;

pub use api_result::{ApiResult, ResultType};
pub use routes::router;

use crate::dashboard::{Counts, Dashboard, DashboardDto};
use crate::log::LogService;
use crate::maker::MakerService;
use crate::plugin::PluginManager;
use crate::scenario::ScenarioService;
use crate::sender::SenderService;

/// Directories used by the server.
#[derive(Debug, Clone)]
pub struct Config {
    /// Maker, sender, log and scenario definitions (`*.json`).
    pub data_root: PathBuf,
    /// External plugin libraries.
    pub plugin_root: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginDto {
    pub name: String,
    pub version: String,
    pub provider: String,
    /// Library file name; `null` for the built-in plugin.
    pub filename: Option<String>,
    /// Number of makers and senders created from this plugin.
    #[serde(rename = "ref")]
    pub refs: usize,
}

pub struct AppState {
    plugins: Arc<PluginManager>,
    makers: Arc<MakerService>,
    senders: Arc<SenderService>,
    logs: Arc<LogService>,
    scenarios: Arc<ScenarioService>,
    dashboard: Dashboard,
}

impl AppState {
    /// Loads plugins and stored definitions, and starts unpaused logs.
    pub fn start(config: &Config) -> anyhow::Result<Arc<Self>> {
        Self::start_with_plugins(config, Vec::new())
    }

    /// Like [`AppState::start`], registering additional linked-in plugins
    /// after the built-in one.
    pub fn start_with_plugins(config: &Config, extra: Vec<PluginDefinition>) -> anyhow::Result<Arc<Self>> {
        for dir in [&config.data_root, &config.plugin_root] {
            std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let plugins = Arc::new(PluginManager::new(config.plugin_root.clone()));
        for definition in std::iter::once(logmaker_default_plugin::definition()).chain(extra) {
            plugins.register(definition).map_err(anyhow::Error::msg)?;
        }
        plugins.load_directory();

        let makers = Arc::new(MakerService::new(Arc::clone(&plugins), &config.data_root));
        makers.load().context("cannot load makers")?;
        let senders = Arc::new(SenderService::new(Arc::clone(&plugins), &config.data_root));
        senders.load().context("cannot load senders")?;
        let logs = Arc::new(LogService::new(
            Arc::clone(&makers),
            Arc::clone(&senders),
            &config.data_root,
        ));
        logs.load().context("cannot load logs")?;
        let scenarios = Arc::new(ScenarioService::new(
            Arc::clone(&makers),
            Arc::clone(&senders),
            Arc::clone(&logs),
            &config.data_root,
        ));
        scenarios.load().context("cannot load scenarios")?;

        Ok(Arc::new(Self {
            plugins,
            makers,
            senders,
            logs,
            scenarios,
            dashboard: Dashboard::default(),
        }))
    }

    /// Stops scenarios and logs, then closes senders (flushing buffered data).
    pub fn shutdown(&self) {
        self.scenarios.shutdown();
        self.logs.shutdown();
        self.senders.shutdown();
        self.makers.shutdown();
    }

    fn dashboard(&self) -> DashboardDto {
        let counts = Counts {
            maker: self.makers.count(),
            sender: self.senders.count(),
            plugin: self.plugins.count(),
            scenario: self.scenarios.count(),
        };
        self.dashboard.build(&self.logs.list(), counts)
    }

    fn plugin_list(&self) -> Vec<PluginDto> {
        self.plugins
            .list()
            .into_iter()
            .map(|p| PluginDto {
                refs: self.makers.plugin_usage(&p.info.id).0 + self.senders.plugin_usage(&p.info.id).0,
                name: p.info.id,
                version: p.info.version,
                provider: p.info.provider,
                filename: p.filename,
            })
            .collect()
    }

    fn upload_plugin(&self, file_name: Option<&str>, bytes: &[u8]) -> ApiResult {
        match self.plugins.install(file_name, bytes) {
            Ok(_) => ApiResult::success("Plugin uploaded successfully"),
            Err(e) => ApiResult::error(format!("Plugin upload failed ({e})")),
        }
    }

    /// Deletes an external plugin together with its makers and senders,
    /// unless any of them is used by a log or scenario.
    fn delete_plugin(&self, id: &str) -> ApiResult {
        if let Err(e) = self.plugins.check_removable(id) {
            return ApiResult::error(format!("Plugin deletion failed ({e})"));
        }
        if self.makers.plugin_usage(id).1 || self.senders.plugin_usage(id).1 {
            return ApiResult::error("Plugin deletion failed (plugin is in use)");
        }
        self.makers.delete_by_plugin(id);
        self.senders.delete_by_plugin(id);
        match self.plugins.uninstall(id) {
            Ok(()) => ApiResult::success("Successfully deleted plugin"),
            Err(e) => ApiResult::error(format!("Plugin deletion failed ({e})")),
        }
    }
}
