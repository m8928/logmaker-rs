//! Maker registry: configured value generators referenced by log templates.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use indexmap::{IndexMap, IndexSet};
use logmaker_plugin_api::{Args, Maker, PluginError, check_args};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::api_result::ApiResult;
use crate::plugin::PluginManager;
use crate::refs::{Ref, RefCounted};
use crate::storage::{Unloaded, entry_name, load_json, null_as_default, save_json};

pub type MakerRef = Ref<MakerSlot>;

/// Body of maker create/update/import requests and entries of `makers.json`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MakerRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "type", default)]
    pub type_name: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub args: Args,
    #[serde(default)]
    pub reg_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MakerDto {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub args: Args,
    pub sample: String,
    pub size: u64,
    #[serde(rename = "ref")]
    pub refs: usize,
    pub reg_time: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredMaker<'a> {
    name: &'a str,
    #[serde(rename = "type")]
    type_name: &'a str,
    args: Args,
    reg_time: i64,
}

struct MakerState {
    args: Args,
    maker: Box<dyn Maker>,
}

pub struct MakerSlot {
    name: String,
    type_name: String,
    plugin_id: String,
    reg_time: i64,
    /// Replaced as a whole on update; callers clone the `Arc` and generate
    /// without holding the lock.
    state: RwLock<Arc<MakerState>>,
    refs: AtomicUsize,
}

impl RefCounted for MakerSlot {
    fn ref_counter(&self) -> &AtomicUsize {
        &self.refs
    }
}

impl MakerSlot {
    pub fn get_data(&self) -> String {
        self.state().maker.get_data()
    }

    fn state(&self) -> Arc<MakerState> {
        Arc::clone(&self.state.read())
    }
}

pub fn now_epoch_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn args_text(args: &Args) -> String {
    serde_json::Value::Object(args.clone()).to_string()
}

pub struct MakerService {
    plugins: Arc<PluginManager>,
    path: PathBuf,
    makers: Mutex<IndexMap<String, Arc<MakerSlot>>>,
    unloaded: Unloaded,
    persist_lock: Mutex<()>,
}

impl MakerService {
    pub fn new(plugins: Arc<PluginManager>, data_root: &Path) -> Self {
        Self {
            plugins,
            path: data_root.join("makers.json"),
            makers: Mutex::new(IndexMap::new()),
            unloaded: Unloaded::default(),
            persist_lock: Mutex::new(()),
        }
    }

    pub fn load(&self) -> std::io::Result<()> {
        let stored: Vec<serde_json::Value> = load_json(&self.path)?;
        for entry in stored {
            let result = match MakerRequest::deserialize(&entry) {
                Ok(request) => self.register(request, true),
                Err(e) => ApiResult::error(e.to_string()),
            };
            if !result.is_success() {
                let reason = result.message.unwrap_or_default();
                tracing::warn!(
                    "cannot load stored maker {}: {reason}; keeping it in storage",
                    entry_name(&entry)
                );
                self.unloaded.keep(entry);
            }
        }
        tracing::info!(
            "loaded {} maker(s), {} kept unloaded",
            self.count(),
            self.unloaded.len()
        );
        Ok(())
    }

    fn snapshot(&self) -> Vec<Arc<MakerSlot>> {
        self.makers.lock().values().cloned().collect()
    }

    /// All makers, newest first, each with a freshly generated sample.
    pub fn list(&self) -> Vec<MakerDto> {
        let mut makers: Vec<MakerDto> = self
            .snapshot()
            .iter()
            .map(|slot| {
                let state = slot.state();
                MakerDto {
                    name: slot.name.clone(),
                    type_name: slot.type_name.clone(),
                    args: state.args.clone(),
                    sample: state.maker.get_data(),
                    size: state.maker.size(),
                    refs: slot.refs(),
                    reg_time: slot.reg_time,
                }
            })
            .collect();
        makers.sort_by(|a, b| b.reg_time.cmp(&a.reg_time));
        makers
    }

    pub fn count(&self) -> usize {
        self.makers.lock().len()
    }

    pub fn exists(&self, name: &str) -> bool {
        self.makers.lock().contains_key(name)
    }

    pub fn create(&self, request: MakerRequest) -> ApiResult {
        if request.name.as_deref().unwrap_or_default().is_empty() {
            return ApiResult::validation("name", "Name field value is required");
        }
        self.register(request, false)
    }

    pub fn import(&self, requests: Vec<MakerRequest>) -> Vec<ApiResult> {
        requests.into_iter().map(|r| self.create(r)).collect()
    }

    pub fn import_file(&self, bytes: &[u8]) -> Vec<ApiResult> {
        match serde_json::from_slice::<Vec<MakerRequest>>(bytes) {
            Ok(requests) => self.import(requests),
            Err(_) => vec![ApiResult::error("Maker file import failed")],
        }
    }

    fn new_instance(&self, type_name: &str, name: &str, args: &Args) -> Result<(String, Box<dyn Maker>), ApiResult> {
        let Some((plugin_id, factory)) = self.plugins.maker_factory(type_name) else {
            return Err(ApiResult::error(format!("{type_name} is an unavailable maker type")));
        };
        let maker = check_args(&factory.args(), args)
            .and_then(|()| factory.create(name, args))
            .map_err(|e| match e {
                PluginError::InvalidArgument(_) => {
                    ApiResult::error(format!("Invalid maker argument ({})", args_text(args)))
                }
                PluginError::Failed(message) => ApiResult::error(format!("Maker creation failed ({message})")),
            })?;
        Ok((plugin_id, maker))
    }

    fn register(&self, request: MakerRequest, from_storage: bool) -> ApiResult {
        let name = request.name.unwrap_or_default();
        let type_name = request.type_name.unwrap_or_else(|| "null".into());
        let in_use = || ApiResult::error(format!("{name} is the maker name already in use"));
        if self.exists(&name) {
            return in_use();
        }
        let (plugin_id, maker) = match self.new_instance(&type_name, &name, &request.args) {
            Ok(created) => created,
            Err(result) => return result,
        };
        let reg_time = match request.reg_time {
            Some(t) if from_storage && t > 0 => t,
            _ => now_epoch_seconds(),
        };
        let slot = Arc::new(MakerSlot {
            name: name.clone(),
            type_name,
            plugin_id,
            reg_time,
            state: RwLock::new(Arc::new(MakerState {
                args: request.args,
                maker,
            })),
            refs: AtomicUsize::new(0),
        });
        {
            let mut makers = self.makers.lock();
            if makers.contains_key(&name) {
                return in_use();
            }
            makers.insert(name.clone(), slot);
        }
        if !from_storage {
            self.unloaded.forget(&name);
            if let Err(result) = self.persist() {
                return result;
            }
        }
        ApiResult::success("Successful maker registration")
    }

    /// Replaces the maker's arguments; logs using it see the new values at once.
    pub fn update(&self, name: &str, request: MakerRequest) -> ApiResult {
        let Some(slot) = self.makers.lock().get(name).cloned() else {
            return ApiResult::error("Update maker failed");
        };
        let maker = match self.new_instance(&slot.type_name, name, &request.args) {
            Ok((_, maker)) => maker,
            Err(result) => return result,
        };
        let previous = std::mem::replace(
            &mut *slot.state.write(),
            Arc::new(MakerState {
                args: request.args,
                maker,
            }),
        );
        drop(previous);
        match self.persist() {
            Ok(()) => ApiResult::success("Successfully updated maker"),
            Err(result) => result,
        }
    }

    pub fn delete(&self, name: &str) -> ApiResult {
        let removed = {
            let mut makers = self.makers.lock();
            let Some(slot) = makers.get(name) else {
                return ApiResult::error("Maker does not exist");
            };
            if slot.refs() > 0 {
                return ApiResult::error("Maker is currently in use");
            }
            makers.shift_remove(name)
        };
        drop(removed);
        match self.persist() {
            Ok(()) => ApiResult::success("Successfully deleted maker"),
            Err(result) => result,
        }
    }

    /// Acquires usage handles for `names`, or returns the missing names.
    pub fn acquire_all(&self, names: &[String]) -> Result<Vec<MakerRef>, Vec<String>> {
        let makers = self.makers.lock();
        let unique: IndexSet<&String> = names.iter().collect();
        let missing: Vec<String> = unique
            .iter()
            .filter(|n| !makers.contains_key(n.as_str()))
            .map(|n| (*n).clone())
            .collect();
        if !missing.is_empty() {
            return Err(missing);
        }
        Ok(unique.iter().map(|n| Ref::acquire(&makers[n.as_str()])).collect())
    }

    /// Current slot without acquiring a reference (for previews).
    pub fn peek(&self, name: &str) -> Option<Arc<MakerSlot>> {
        self.makers.lock().get(name).cloned()
    }

    /// (number of makers from the plugin, whether any is in use)
    pub fn plugin_usage(&self, plugin_id: &str) -> (usize, bool) {
        let makers = self.makers.lock();
        let owned = makers.values().filter(|m| m.plugin_id == plugin_id);
        owned.fold((0, false), |(count, used), m| (count + 1, used || m.refs() > 0))
    }

    pub fn delete_by_plugin(&self, plugin_id: &str) {
        let names: Vec<String> = self
            .makers
            .lock()
            .values()
            .filter(|m| m.plugin_id == plugin_id)
            .map(|m| m.name.clone())
            .collect();
        for name in names {
            self.delete(&name);
        }
    }

    pub fn shutdown(&self) {
        let makers = std::mem::take(&mut *self.makers.lock());
        drop(makers);
    }

    fn persist(&self) -> Result<(), ApiResult> {
        let _guard = self.persist_lock.lock();
        let slots = self.snapshot();
        let stored: Vec<StoredMaker> = slots
            .iter()
            .map(|slot| StoredMaker {
                name: &slot.name,
                type_name: &slot.type_name,
                args: slot.state().args.clone(),
                reg_time: slot.reg_time,
            })
            .collect();
        self.unloaded
            .merge(&stored)
            .and_then(|entries| save_json(&self.path, &entries))
            .map_err(|e| {
                tracing::error!("failed to save {}: {e}", self.path.display());
                ApiResult::error(format!("Failed to save maker storage ({e})"))
            })
    }
}
