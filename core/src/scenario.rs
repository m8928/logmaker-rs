//! Scenarios: ordered log steps, each with its own senders, repeat count,
//! delay and field overrides. Shared variables take one maker value per loop
//! and reuse it in every step (e.g. the same user across login and logout).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Duration;

use indexmap::{IndexMap, IndexSet};
use parking_lot::Mutex;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::api_result::ApiResult;
use crate::log::{LogConfig, LogService};
use crate::maker::{MakerRef, MakerService};
use crate::sender::{SenderRef, SenderService};
use crate::storage::{Unloaded, entry_name, load_json, null_as_default, save_json};
use crate::template::render_references;
use crate::worker::{STOP_TIMEOUT, StopSignal, Worker};

const NOT_FOUND: &str = "Scenario does not exist";
const BUSY: &str = "Scenario is busy";

fn default_interval_min() -> i64 {
    1_000
}

fn default_interval_max() -> i64 {
    5_000
}

fn default_repeat() -> i64 {
    1
}

/// Scenario definition, as sent by clients and stored in `scenarios.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioConfig {
    #[serde(default, deserialize_with = "null_as_default")]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Variable name → maker name. Each loop draws one value per variable.
    #[serde(default, deserialize_with = "null_as_default")]
    pub shared_variables: IndexMap<String, String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub steps: Vec<ScenarioStep>,
    /// Pause between loops, chosen uniformly in `[min, max]`.
    #[serde(default = "default_interval_min", deserialize_with = "null_as_default")]
    pub interval_min_ms: i64,
    #[serde(default = "default_interval_max", deserialize_with = "null_as_default")]
    pub interval_max_ms: i64,
    /// Number of loops; 0 runs until stopped.
    #[serde(default, deserialize_with = "null_as_default")]
    pub loop_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioStep {
    #[serde(default, deserialize_with = "null_as_default")]
    pub log_name: String,
    #[serde(default = "default_repeat", deserialize_with = "null_as_default")]
    pub repeat: i64,
    /// Delay before each repetition, chosen uniformly in `[min, max]`.
    #[serde(default, deserialize_with = "null_as_default")]
    pub delay_min_ms: i64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub delay_max_ms: i64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub senders: Vec<String>,
    /// Maker name → literal value; may reference shared variables (`${var}`).
    #[serde(default, deserialize_with = "null_as_default")]
    pub overrides: IndexMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDto {
    #[serde(flatten)]
    pub config: ScenarioConfig,
    pub status: bool,
    pub count: u64,
    pub current_step: i64,
    pub current_loop: i64,
    pub total_steps: usize,
    pub step_counts: Option<Vec<u64>>,
}

struct ScenarioRun {
    count: AtomicU64,
    /// 1-based; -1 before the first step.
    current_step: AtomicI64,
    current_loop: AtomicI64,
    step_counts: Vec<AtomicU64>,
    worker: Mutex<Option<Worker>>,
}

impl ScenarioRun {
    fn new(steps: usize) -> Self {
        Self {
            count: AtomicU64::new(0),
            current_step: AtomicI64::new(-1),
            current_loop: AtomicI64::new(0),
            step_counts: (0..steps).map(|_| AtomicU64::new(0)).collect(),
            worker: Mutex::new(None),
        }
    }

    fn is_active(&self) -> bool {
        self.worker.lock().as_ref().is_some_and(|w| !w.is_finished())
    }

    fn stop(&self) -> bool {
        self.worker.lock().as_ref().is_none_or(|w| w.stop(STOP_TIMEOUT))
    }
}

#[derive(Default)]
struct State {
    configs: IndexMap<String, Arc<ScenarioConfig>>,
    runs: HashMap<String, Arc<ScenarioRun>>,
    /// Scenarios being stopped for an update or delete.
    busy: HashSet<String>,
}

pub struct ScenarioService {
    makers: Arc<MakerService>,
    senders: Arc<SenderService>,
    logs: Arc<LogService>,
    path: PathBuf,
    state: Mutex<State>,
    unloaded: Unloaded,
    persist_lock: Mutex<()>,
}

impl ScenarioService {
    pub fn new(
        makers: Arc<MakerService>,
        senders: Arc<SenderService>,
        logs: Arc<LogService>,
        data_root: &Path,
    ) -> Self {
        Self {
            makers,
            senders,
            logs,
            path: data_root.join("scenarios.json"),
            state: Mutex::new(State::default()),
            unloaded: Unloaded::default(),
            persist_lock: Mutex::new(()),
        }
    }

    pub fn load(&self) -> std::io::Result<()> {
        let stored: Vec<serde_json::Value> = load_json(&self.path)?;
        for entry in stored {
            let result = match ScenarioConfig::deserialize(&entry) {
                Ok(config) => self.register(config, true),
                Err(e) => ApiResult::error(e.to_string()),
            };
            if !result.is_success() {
                let reason = result.message.unwrap_or_default();
                tracing::warn!(
                    "cannot load stored scenario {}: {reason}; keeping it in storage",
                    entry_name(&entry)
                );
                self.unloaded.keep(entry);
            }
        }
        tracing::info!(
            "loaded {} scenario(s), {} kept unloaded",
            self.count(),
            self.unloaded.len()
        );
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.state.lock().configs.len()
    }

    pub fn list(&self) -> Vec<ScenarioDto> {
        let mut state = self.state.lock();
        state.runs.retain(|_, run| run.is_active());
        state
            .configs
            .values()
            .map(|config| {
                let run = state.runs.get(&config.name);
                ScenarioDto {
                    config: (**config).clone(),
                    status: run.is_some(),
                    count: run.map_or(0, |r| r.count.load(Ordering::Relaxed)),
                    current_step: run.map_or(0, |r| r.current_step.load(Ordering::Relaxed)),
                    current_loop: run.map_or(0, |r| r.current_loop.load(Ordering::Relaxed)),
                    total_steps: config.steps.len(),
                    step_counts: run.map(|r| r.step_counts.iter().map(|c| c.load(Ordering::Relaxed)).collect()),
                }
            })
            .collect()
    }

    pub fn create(&self, config: ScenarioConfig) -> ApiResult {
        if config.name.trim().is_empty() {
            return ApiResult::validation("name", "Name field value is required");
        }
        self.register(config, false)
    }

    fn register(&self, config: ScenarioConfig, from_storage: bool) -> ApiResult {
        if config.name.trim().is_empty() {
            return ApiResult::error("Scenario name is required");
        }
        let name = config.name.clone();
        {
            let mut state = self.state.lock();
            if state.busy.contains(&config.name) {
                return ApiResult::error(BUSY);
            }
            if state.configs.contains_key(&config.name) {
                return ApiResult::error(format!("{} is the scenario name already in use", config.name));
            }
            if let Some(error) = self.validate_references(&config) {
                return ApiResult::error(error);
            }
            state.configs.insert(config.name.clone(), Arc::new(config));
        }
        if !from_storage {
            self.unloaded.forget(&name);
            if let Err(result) = self.persist() {
                return result;
            }
        }
        ApiResult::success("Successful scenario registration")
    }

    /// Replaces a scenario definition, restarting it if it was running.
    pub fn update(&self, name: &str, mut config: ScenarioConfig) -> ApiResult {
        config.name = name.to_owned();
        if !self.state.lock().configs.contains_key(name) {
            return ApiResult::error(NOT_FOUND);
        }
        if let Some(error) = self.validate_references(&config) {
            return ApiResult::error(error);
        }
        let (run, was_running) = {
            let mut state = self.state.lock();
            if !state.configs.contains_key(name) {
                return ApiResult::error(NOT_FOUND);
            }
            if !state.busy.insert(name.to_owned()) {
                return ApiResult::error(BUSY);
            }
            let run = state.runs.remove(name);
            let active = run.as_ref().is_some_and(|r| r.is_active());
            (run, active)
        };
        if let Some(run) = run.filter(|_| was_running) {
            if !run.stop() {
                let mut state = self.state.lock();
                state.runs.entry(name.to_owned()).or_insert(run);
                state.busy.remove(name);
                return ApiResult::error("Scenario did not stop before update");
            }
        }
        {
            let mut state = self.state.lock();
            match state.configs.get_mut(name) {
                Some(slot) => *slot = Arc::new(config),
                None => {
                    state.busy.remove(name);
                    return ApiResult::error(NOT_FOUND);
                }
            }
        }
        let saved = self.persist();
        self.state.lock().busy.remove(name);
        if let Err(result) = saved {
            return result;
        }
        if was_running {
            let restarted = self.start(name);
            if !restarted.is_success() {
                return restarted;
            }
        }
        ApiResult::success("Successfully updated scenario")
    }

    pub fn delete(&self, name: &str) -> ApiResult {
        let (index, config, run) = {
            let mut state = self.state.lock();
            if !state.busy.insert(name.to_owned()) {
                return ApiResult::error(BUSY);
            }
            let Some((index, _, config)) = state.configs.shift_remove_full(name) else {
                state.busy.remove(name);
                return ApiResult::error(NOT_FOUND);
            };
            let run = state.runs.remove(name);
            (index, config, run)
        };
        if let Some(run) = run {
            if !run.stop() {
                let mut state = self.state.lock();
                if !state.configs.contains_key(name) {
                    let index = index.min(state.configs.len());
                    state.configs.shift_insert(index, name.to_owned(), config);
                }
                state.runs.entry(name.to_owned()).or_insert(run);
                state.busy.remove(name);
                return ApiResult::error("Scenario did not stop before deletion");
            }
        }
        let saved = self.persist();
        self.state.lock().busy.remove(name);
        match saved {
            Ok(()) => ApiResult::success("Successfully deleted scenario"),
            Err(result) => result,
        }
    }

    pub fn start(&self, name: &str) -> ApiResult {
        let mut state = self.state.lock();
        if state.busy.contains(name) {
            return ApiResult::error(BUSY);
        }
        let Some(config) = state.configs.get(name).cloned() else {
            return ApiResult::error(NOT_FOUND);
        };
        if let Some(error) = self.validate_references(&config) {
            return ApiResult::error(error);
        }
        if state.runs.get(name).is_some_and(|r| r.is_active()) {
            return ApiResult::error("Scenario is already running");
        }

        let maker_names: Vec<String> = config.shared_variables.values().cloned().collect();
        let makers = match self.makers.acquire_all(&maker_names) {
            Ok(refs) => maker_names
                .iter()
                .cloned()
                .collect::<IndexSet<_>>()
                .into_iter()
                .zip(refs)
                .collect(),
            Err(missing) => return ApiResult::error(format!("Scenario references unknown maker: {}", missing[0])),
        };
        let sender_names: Vec<String> = config.steps.iter().flat_map(|s| s.senders.iter().cloned()).collect();
        let senders = match self.senders.acquire_all(&sender_names) {
            Ok(refs) => sender_names
                .iter()
                .cloned()
                .collect::<IndexSet<_>>()
                .into_iter()
                .zip(refs)
                .collect(),
            Err(missing) => return ApiResult::error(format!("Scenario references unknown sender: {}", missing[0])),
        };

        let run = Arc::new(ScenarioRun::new(config.steps.len()));
        let execution = Execution {
            config,
            run: Arc::clone(&run),
            logs: Arc::clone(&self.logs),
            makers,
            senders,
        };
        match Worker::spawn(&format!("scenario-{name}"), move |stop| execution.run(stop)) {
            Ok(worker) => *run.worker.lock() = Some(worker),
            Err(e) => {
                tracing::error!("failed to start scenario thread {name}: {e}");
                return ApiResult::error("Scenario thread start failed");
            }
        }
        state.runs.insert(name.to_owned(), run);
        ApiResult::success("Scenario started")
    }

    pub fn stop(&self, name: &str) -> ApiResult {
        let run = {
            let mut state = self.state.lock();
            if state.busy.contains(name) {
                return ApiResult::error(BUSY);
            }
            let Some(run) = state.runs.remove(name) else {
                return ApiResult::error("Scenario is not running");
            };
            state.busy.insert(name.to_owned());
            run
        };
        let stopped = run.stop();
        let mut state = self.state.lock();
        if !stopped {
            state.runs.entry(name.to_owned()).or_insert(run);
        }
        state.busy.remove(name);
        if stopped {
            ApiResult::success("Scenario stopped")
        } else {
            ApiResult::error("Scenario did not stop")
        }
    }

    pub fn shutdown(&self) {
        let runs: Vec<_> = self.state.lock().runs.drain().collect();
        for (name, run) in runs {
            if !run.stop() {
                tracing::warn!("scenario {name} did not stop within {STOP_TIMEOUT:?}");
            }
        }
    }

    fn validate_references(&self, config: &ScenarioConfig) -> Option<String> {
        for (index, step) in config.steps.iter().enumerate() {
            if step.log_name.trim().is_empty() {
                return Some(format!("Scenario step {} log is required", index + 1));
            }
            if self.logs.get(&step.log_name).is_none() {
                return Some(format!("Scenario references unknown log: {}", step.log_name));
            }
            for sender in &step.senders {
                if sender.trim().is_empty() {
                    return Some("Scenario step sender is required".into());
                }
                if !self.senders.exists(sender) {
                    return Some(format!("Scenario references unknown sender: {sender}"));
                }
            }
        }
        for (variable, maker) in &config.shared_variables {
            if maker.trim().is_empty() {
                return Some(format!("Scenario shared variable {variable} maker is required"));
            }
            if !self.makers.exists(maker) {
                return Some(format!("Scenario references unknown maker: {maker}"));
            }
        }
        None
    }

    fn persist(&self) -> Result<(), ApiResult> {
        let _guard = self.persist_lock.lock();
        let configs: Vec<Arc<ScenarioConfig>> = self.state.lock().configs.values().cloned().collect();
        let stored: Vec<&ScenarioConfig> = configs.iter().map(|c| c.as_ref()).collect();
        self.unloaded
            .merge(&stored)
            .and_then(|entries| save_json(&self.path, &entries))
            .map_err(|e| {
                tracing::error!("failed to save {}: {e}", self.path.display());
                ApiResult::error(format!("Failed to save scenario storage ({e})"))
            })
    }
}

/// Everything a running scenario thread owns. The maker and sender handles
/// keep them from being deleted until the run ends.
struct Execution {
    config: Arc<ScenarioConfig>,
    run: Arc<ScenarioRun>,
    logs: Arc<LogService>,
    makers: IndexMap<String, MakerRef>,
    senders: IndexMap<String, SenderRef>,
}

fn random_between(min: i64, max: i64) -> i64 {
    if min >= max {
        min
    } else {
        rand::rng().random_range(min..=max)
    }
}

/// Sleeps a random time in `[min, max]` ms; `false` if stopped.
fn sleep_between(stop: &StopSignal, min: i64, max: i64) -> bool {
    if stop.is_stopped() {
        return false;
    }
    match u64::try_from(random_between(min, max)) {
        Ok(ms) if ms > 0 => stop.sleep(Duration::from_millis(ms)),
        _ => true,
    }
}

/// Renders a step's log. Precedence per template name: shared variable, then
/// step override (with `${var}` references resolved), then the log's maker.
fn render_step(config: &LogConfig, step: &ScenarioStep, vars: &IndexMap<String, String>) -> String {
    let template = config.template();
    let values: Vec<String> = template
        .names()
        .iter()
        .enumerate()
        .map(|(index, name)| {
            if let Some(value) = vars.get(name) {
                value.clone()
            } else if let Some(value) = step.overrides.get(name) {
                if vars.is_empty() || !value.contains('$') {
                    value.clone()
                } else {
                    render_references(value, vars)
                }
            } else {
                config.maker_value(index)
            }
        })
        .collect();
    template.render(&values)
}

impl Execution {
    fn more_loops(&self, done: i64) -> bool {
        self.config.loop_count == 0 || done < self.config.loop_count
    }

    fn run(&self, stop: &StopSignal) {
        let mut done = 0;
        while !stop.is_stopped() && self.more_loops(done) {
            self.run.current_loop.store(done + 1, Ordering::Relaxed);
            let vars: IndexMap<String, String> = self
                .config
                .shared_variables
                .iter()
                .filter_map(|(variable, maker)| self.makers.get(maker).map(|m| (variable.clone(), m.get_data())))
                .collect();
            for (index, step) in self.config.steps.iter().enumerate() {
                if stop.is_stopped() {
                    break;
                }
                self.run.current_step.store(index as i64 + 1, Ordering::Relaxed);
                self.run_step(index, step, &vars, stop);
            }
            done += 1;
            if !stop.is_stopped() && self.more_loops(done) {
                sleep_between(stop, self.config.interval_min_ms, self.config.interval_max_ms);
            }
        }
    }

    fn run_step(&self, index: usize, step: &ScenarioStep, vars: &IndexMap<String, String>, stop: &StopSignal) {
        let Some(log) = self.logs.get(&step.log_name) else {
            tracing::warn!(
                "scenario {} step {} references unknown log {}",
                self.config.name,
                index + 1,
                step.log_name
            );
            return;
        };
        let names: IndexSet<&String> = step.senders.iter().collect();
        let senders: Vec<&SenderRef> = names.into_iter().filter_map(|name| self.senders.get(name)).collect();
        for _ in 0..step.repeat.max(0) {
            if !sleep_between(stop, step.delay_min_ms, step.delay_max_ms) {
                return;
            }
            let data = render_step(&log.config(), step, vars);
            for sender in &senders {
                sender.deliver(&data);
            }
            self.run.count.fetch_add(1, Ordering::Relaxed);
            self.run.step_counts[index].fetch_add(1, Ordering::Relaxed);
        }
    }
}
