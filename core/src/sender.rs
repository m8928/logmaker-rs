//! Sender registry: configured delivery targets with delivery counters.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use indexmap::{IndexMap, IndexSet};
use logmaker_plugin_api::{Args, PluginError, Sender, check_args};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::api_result::ApiResult;
use crate::maker::now_epoch_seconds;
use crate::plugin::PluginManager;
use crate::refs::{Ref, RefCounted};
use crate::storage::{Unloaded, entry_name, load_json, null_as_default, save_json};

pub type SenderRef = Ref<SenderSlot>;

/// Minimum seconds between two logged delivery errors of one sender.
const ERROR_LOG_INTERVAL_SECS: u64 = 10;

/// Body of sender create/update/import requests and entries of `senders.json`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SenderRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "type", default)]
    pub type_name: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub args: Args,
    /// Maximum number of lines to deliver; 0 means unlimited.
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub reg_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SenderDto {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub args: Args,
    pub limit: u64,
    #[serde(rename = "ref")]
    pub refs: usize,
    pub count: u64,
    pub bytes: u64,
    pub bytes_per_sec: u64,
    pub reg_time: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSender<'a> {
    name: &'a str,
    #[serde(rename = "type")]
    type_name: &'a str,
    args: Args,
    limit: u64,
    reg_time: i64,
}

struct SenderState {
    args: Args,
    sender: Box<dyn Sender>,
}

/// Bytes delivered in the current second.
#[derive(Default)]
struct SecondBytes {
    second: u64,
    bytes: u64,
}

pub struct SenderSlot {
    name: String,
    type_name: String,
    plugin_id: String,
    reg_time: i64,
    /// Replaced as a whole on update; deliveries clone the `Arc` and send
    /// without holding the lock, so an update never waits for a slow send.
    state: RwLock<Arc<SenderState>>,
    refs: AtomicUsize,
    count: AtomicU64,
    bytes: AtomicU64,
    limit: AtomicU64,
    current_second: Mutex<SecondBytes>,
    last_error_log: AtomicU64,
}

impl RefCounted for SenderSlot {
    fn ref_counter(&self) -> &AtomicUsize {
        &self.refs
    }
}

fn epoch_seconds() -> u64 {
    now_epoch_seconds().max(0) as u64
}

impl SenderSlot {
    fn limit_reached(&self) -> bool {
        let limit = self.limit.load(Ordering::Relaxed);
        limit > 0 && self.count.load(Ordering::Relaxed) >= limit
    }

    /// Sends one line unless the limit is reached. Successful sends update the
    /// counters; failures are logged at most every few seconds.
    pub fn deliver(&self, data: &str) {
        if self.limit_reached() {
            return;
        }
        let state = Arc::clone(&self.state.read());
        let result = state.sender.send(data);
        match result {
            Ok(()) => {
                self.count.fetch_add(1, Ordering::Relaxed);
                self.add_bytes(data.len() as u64);
            }
            Err(e) => self.log_error(&e),
        }
    }

    fn log_error(&self, error: &PluginError) {
        let now = epoch_seconds();
        let last = self.last_error_log.load(Ordering::Relaxed);
        if now.saturating_sub(last) >= ERROR_LOG_INTERVAL_SECS
            && self
                .last_error_log
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            tracing::error!("failed to send data to sender {}: {error}", self.name);
        }
    }

    fn add_bytes(&self, size: u64) {
        self.bytes.fetch_add(size, Ordering::Relaxed);
        let now = epoch_seconds();
        let mut current = self.current_second.lock();
        if current.second == now {
            current.bytes += size;
        } else {
            *current = SecondBytes {
                second: now,
                bytes: size,
            };
        }
    }

    /// Bytes delivered in the current second, or the previous one if nothing
    /// was sent yet this second; 0 after an idle second.
    fn bytes_per_sec(&self) -> u64 {
        let current = self.current_second.lock();
        if current.second == 0 || epoch_seconds().saturating_sub(current.second) > 1 {
            0
        } else {
            current.bytes
        }
    }
}

pub struct SenderService {
    plugins: Arc<PluginManager>,
    path: PathBuf,
    senders: Mutex<IndexMap<String, Arc<SenderSlot>>>,
    unloaded: Unloaded,
    persist_lock: Mutex<()>,
}

impl SenderService {
    pub fn new(plugins: Arc<PluginManager>, data_root: &Path) -> Self {
        Self {
            plugins,
            path: data_root.join("senders.json"),
            senders: Mutex::new(IndexMap::new()),
            unloaded: Unloaded::default(),
            persist_lock: Mutex::new(()),
        }
    }

    pub fn load(&self) -> std::io::Result<()> {
        let stored: Vec<serde_json::Value> = load_json(&self.path)?;
        for entry in stored {
            let result = match SenderRequest::deserialize(&entry) {
                Ok(request) => self.register(request, true),
                Err(e) => ApiResult::error(e.to_string()),
            };
            if !result.is_success() {
                let reason = result.message.unwrap_or_default();
                tracing::warn!(
                    "cannot load stored sender {}: {reason}; keeping it in storage",
                    entry_name(&entry)
                );
                self.unloaded.keep(entry);
            }
        }
        tracing::info!(
            "loaded {} sender(s), {} kept unloaded",
            self.count(),
            self.unloaded.len()
        );
        Ok(())
    }

    fn snapshot(&self) -> Vec<Arc<SenderSlot>> {
        self.senders.lock().values().cloned().collect()
    }

    /// All senders, newest first.
    pub fn list(&self) -> Vec<SenderDto> {
        let mut senders: Vec<SenderDto> = self
            .snapshot()
            .iter()
            .map(|slot| SenderDto {
                name: slot.name.clone(),
                type_name: slot.type_name.clone(),
                args: slot.state.read().args.clone(),
                limit: slot.limit.load(Ordering::Relaxed),
                refs: slot.refs(),
                count: slot.count.load(Ordering::Relaxed),
                bytes: slot.bytes.load(Ordering::Relaxed),
                bytes_per_sec: slot.bytes_per_sec(),
                reg_time: slot.reg_time,
            })
            .collect();
        senders.sort_by(|a, b| b.reg_time.cmp(&a.reg_time));
        senders
    }

    pub fn count(&self) -> usize {
        self.senders.lock().len()
    }

    pub fn exists(&self, name: &str) -> bool {
        self.senders.lock().contains_key(name)
    }

    pub fn create(&self, request: SenderRequest) -> ApiResult {
        if request.name.as_deref().unwrap_or_default().is_empty() {
            return ApiResult::validation("name", "Name field value is required");
        }
        self.register(request, false)
    }

    pub fn import(&self, requests: Vec<SenderRequest>) -> Vec<ApiResult> {
        requests.into_iter().map(|r| self.create(r)).collect()
    }

    pub fn import_file(&self, bytes: &[u8]) -> Vec<ApiResult> {
        match serde_json::from_slice::<Vec<SenderRequest>>(bytes) {
            Ok(requests) => self.import(requests),
            Err(_) => vec![ApiResult::error("Sender file import failed")],
        }
    }

    fn new_instance(&self, type_name: &str, name: &str, args: &Args) -> Result<(String, Box<dyn Sender>), ApiResult> {
        let Some((plugin_id, factory)) = self.plugins.sender_factory(type_name) else {
            return Err(ApiResult::error(format!("{type_name} is an unavailable sender type")));
        };
        let sender = check_args(&factory.args(), args)
            .and_then(|()| factory.create(name, args))
            .map_err(|e| match e {
                PluginError::InvalidArgument(_) => ApiResult::error(format!(
                    "Invalid sender argument ({})",
                    serde_json::Value::Object(args.clone())
                )),
                PluginError::Failed(message) => ApiResult::error(format!("Sender creation failed ({message})")),
            })?;
        Ok((plugin_id, sender))
    }

    fn register(&self, request: SenderRequest, from_storage: bool) -> ApiResult {
        let name = request.name.unwrap_or_default();
        let type_name = request.type_name.unwrap_or_else(|| "null".into());
        let in_use = || ApiResult::error(format!("{name} is the sender name already in use"));
        if self.exists(&name) {
            return in_use();
        }
        let (plugin_id, sender) = match self.new_instance(&type_name, &name, &request.args) {
            Ok(created) => created,
            Err(result) => return result,
        };
        let reg_time = match request.reg_time {
            Some(t) if from_storage && t > 0 => t,
            _ => now_epoch_seconds(),
        };
        let slot = Arc::new(SenderSlot {
            name: name.clone(),
            type_name,
            plugin_id,
            reg_time,
            state: RwLock::new(Arc::new(SenderState {
                args: request.args,
                sender,
            })),
            refs: AtomicUsize::new(0),
            count: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            limit: AtomicU64::new(request.limit.unwrap_or(0).max(0) as u64),
            current_second: Mutex::new(SecondBytes::default()),
            last_error_log: AtomicU64::new(0),
        });
        {
            let mut senders = self.senders.lock();
            if senders.contains_key(&name) {
                return in_use();
            }
            senders.insert(name.clone(), slot);
        }
        if !from_storage {
            self.unloaded.forget(&name);
            if let Err(result) = self.persist() {
                return result;
            }
        }
        ApiResult::success("Successful sender registration")
    }

    /// Replaces the sender's arguments (and limit, when given); logs using it
    /// deliver to the new instance at once. Counters are kept.
    pub fn update(&self, name: &str, request: SenderRequest) -> ApiResult {
        let Some(slot) = self.senders.lock().get(name).cloned() else {
            return ApiResult::error("Update sender failed");
        };
        let sender = match self.new_instance(&slot.type_name, name, &request.args) {
            Ok((_, sender)) => sender,
            Err(result) => return result,
        };
        // An in-flight delivery may still hold the previous instance; it is
        // closed when that delivery returns.
        let previous = std::mem::replace(
            &mut *slot.state.write(),
            Arc::new(SenderState {
                args: request.args,
                sender,
            }),
        );
        drop(previous);
        if let Some(limit) = request.limit {
            slot.limit.store(limit.max(0) as u64, Ordering::Relaxed);
        }
        match self.persist() {
            Ok(()) => ApiResult::success("Successfully updated sender"),
            Err(result) => result,
        }
    }

    pub fn delete(&self, name: &str) -> ApiResult {
        let removed = {
            let mut senders = self.senders.lock();
            let Some(slot) = senders.get(name) else {
                return ApiResult::error("Sender does not exist");
            };
            if slot.refs() > 0 {
                return ApiResult::error("Sender is currently in use");
            }
            senders.shift_remove(name)
        };
        // Closing a sender may flush buffered data; do it outside the lock.
        drop(removed);
        match self.persist() {
            Ok(()) => ApiResult::success("Successfully deleted sender"),
            Err(result) => result,
        }
    }

    /// Acquires usage handles for `names` (duplicates collapse), or returns the
    /// missing names.
    pub fn acquire_all(&self, names: &[String]) -> Result<Vec<SenderRef>, Vec<String>> {
        let senders = self.senders.lock();
        let unique: IndexSet<&String> = names.iter().collect();
        let missing: Vec<String> = unique
            .iter()
            .filter(|n| !senders.contains_key(n.as_str()))
            .map(|n| (*n).clone())
            .collect();
        if !missing.is_empty() {
            return Err(missing);
        }
        Ok(unique.iter().map(|n| Ref::acquire(&senders[n.as_str()])).collect())
    }

    /// (number of senders from the plugin, whether any is in use)
    pub fn plugin_usage(&self, plugin_id: &str) -> (usize, bool) {
        let senders = self.senders.lock();
        let owned = senders.values().filter(|s| s.plugin_id == plugin_id);
        owned.fold((0, false), |(count, used), s| (count + 1, used || s.refs() > 0))
    }

    pub fn delete_by_plugin(&self, plugin_id: &str) {
        let names: Vec<String> = self
            .senders
            .lock()
            .values()
            .filter(|s| s.plugin_id == plugin_id)
            .map(|s| s.name.clone())
            .collect();
        for name in names {
            self.delete(&name);
        }
    }

    pub fn shutdown(&self) {
        let senders = std::mem::take(&mut *self.senders.lock());
        drop(senders);
    }

    fn persist(&self) -> Result<(), ApiResult> {
        let _guard = self.persist_lock.lock();
        let slots = self.snapshot();
        let stored: Vec<StoredSender> = slots
            .iter()
            .map(|slot| StoredSender {
                name: &slot.name,
                type_name: &slot.type_name,
                args: slot.state.read().args.clone(),
                limit: slot.limit.load(Ordering::Relaxed),
                reg_time: slot.reg_time,
            })
            .collect();
        self.unloaded
            .merge(&stored)
            .and_then(|entries| save_json(&self.path, &entries))
            .map_err(|e| {
                tracing::error!("failed to save {}: {e}", self.path.display());
                ApiResult::error(format!("Failed to save sender storage ({e})"))
            })
    }
}
