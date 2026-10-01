//! Registry of plugins and the maker/sender types they provide.
//!
//! The built-in plugin is linked into the server. External plugins are native
//! libraries in the plugin directory, loaded at startup or uploaded at runtime
//! (see [`native`]).

mod native;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use logmaker_plugin_api::{ArgSpec, MakerFactory, PluginDefinition, PluginInfo, SenderFactory};
use parking_lot::RwLock;

/// File extensions accepted as plugin libraries.
pub const LIBRARY_EXTENSIONS: [&str; 3] = ["so", "dylib", "dll"];

struct LoadedPlugin {
    info: PluginInfo,
    /// Library file in the plugin directory; `None` for built-in plugins.
    path: Option<PathBuf>,
    makers: Vec<Arc<dyn MakerFactory>>,
    senders: Vec<Arc<dyn SenderFactory>>,
}

pub struct PluginSummary {
    pub info: PluginInfo,
    pub filename: Option<String>,
}

pub struct TypeInfo {
    pub type_name: String,
    pub args: Vec<ArgSpec>,
}

pub struct PluginManager {
    root: PathBuf,
    plugins: RwLock<Vec<LoadedPlugin>>,
}

fn has_library_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| LIBRARY_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Keeps only the file name, replacing characters outside `[A-Za-z0-9._-]`.
fn sanitize_file_name(original: Option<&str>) -> String {
    let default = format!("plugin.{}", std::env::consts::DLL_EXTENSION);
    let base = original
        .and_then(|name| name.rsplit(['/', '\\']).next())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&default);
    base.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn removable_index(plugins: &[LoadedPlugin], id: &str) -> Result<usize, String> {
    match plugins.iter().position(|p| p.info.id == id) {
        None => Err("plugin does not exist".into()),
        Some(index) if plugins[index].path.is_none() => Err("built-in plugin cannot be deleted".into()),
        Some(index) => Ok(index),
    }
}

fn find_maker(plugins: &[LoadedPlugin], type_name: &str) -> Option<(String, Arc<dyn MakerFactory>)> {
    plugins.iter().find_map(|p| {
        p.makers
            .iter()
            .find(|f| f.type_name() == type_name)
            .map(|f| (p.info.id.clone(), Arc::clone(f)))
    })
}

fn find_sender(plugins: &[LoadedPlugin], type_name: &str) -> Option<(String, Arc<dyn SenderFactory>)> {
    plugins.iter().find_map(|p| {
        p.senders
            .iter()
            .find(|f| f.type_name() == type_name)
            .map(|f| (p.info.id.clone(), Arc::clone(f)))
    })
}

impl PluginManager {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            plugins: RwLock::new(Vec::new()),
        }
    }

    /// Registers a linked-in plugin.
    pub fn register(&self, definition: PluginDefinition) -> Result<(), String> {
        let makers = definition.makers.into_iter().map(Arc::from).collect();
        let senders = definition.senders.into_iter().map(Arc::from).collect();
        self.add(LoadedPlugin {
            info: definition.info,
            path: None,
            makers,
            senders,
        })
    }

    fn add(&self, plugin: LoadedPlugin) -> Result<(), String> {
        let mut plugins = self.plugins.write();
        if plugins.iter().any(|p| p.info.id == plugin.info.id) {
            return Err(format!("plugin {} is already loaded", plugin.info.id));
        }
        for factory in &plugin.makers {
            if find_maker(&plugins, factory.type_name()).is_some() {
                tracing::warn!(
                    "maker type {} from plugin {} is shadowed by an earlier plugin",
                    factory.type_name(),
                    plugin.info.id
                );
            }
        }
        for factory in &plugin.senders {
            if find_sender(&plugins, factory.type_name()).is_some() {
                tracing::warn!(
                    "sender type {} from plugin {} is shadowed by an earlier plugin",
                    factory.type_name(),
                    plugin.info.id
                );
            }
        }
        tracing::info!(
            "loaded plugin {} {} (makers: {}, senders: {})",
            plugin.info.id,
            plugin.info.version,
            plugin
                .makers
                .iter()
                .map(|f| f.type_name())
                .collect::<Vec<_>>()
                .join(", "),
            plugin
                .senders
                .iter()
                .map(|f| f.type_name())
                .collect::<Vec<_>>()
                .join(", "),
        );
        plugins.push(plugin);
        Ok(())
    }

    /// Loads every plugin library in the plugin directory. Failures are logged
    /// and skipped.
    pub fn load_directory(&self) {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::error!("cannot read plugin directory {}: {e}", self.root.display());
                return;
            }
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        paths.sort();
        for path in paths {
            if !has_library_extension(&path) {
                if path.extension().is_some_and(|e| e == "jar") {
                    tracing::warn!(
                        "ignoring Java plugin {}: native plugin libraries are required",
                        path.display()
                    );
                }
                continue;
            }
            if let Err(e) = native::load(&path).and_then(|plugin| self.add(plugin)) {
                tracing::error!("failed to load plugin {}: {e}", path.display());
            }
        }
    }

    /// Stores an uploaded library in the plugin directory and loads it.
    pub fn install(&self, original_name: Option<&str>, bytes: &[u8]) -> Result<PluginInfo, String> {
        let file_name = sanitize_file_name(original_name);
        if !has_library_extension(Path::new(&file_name)) {
            return Err(format!(
                "unsupported file type; expected a native plugin library (.{})",
                LIBRARY_EXTENSIONS.join(", .")
            ));
        }
        let path = self.root.join(format!("{}_{file_name}", uuid::Uuid::new_v4()));
        fs::write(&path, bytes).map_err(|e| e.to_string())?;
        let result = native::load(&path).and_then(|plugin| {
            let info = plugin.info.clone();
            self.add(plugin).map(|()| info)
        });
        if result.is_err() {
            if let Err(e) = fs::remove_file(&path) {
                tracing::error!(
                    "failed to delete plugin file {} after upload failure: {e}",
                    path.display()
                );
            }
        }
        result
    }

    /// Whether `id` is an installed external plugin.
    pub fn check_removable(&self, id: &str) -> Result<(), String> {
        removable_index(&self.plugins.read(), id).map(drop)
    }

    /// Unregisters an external plugin and deletes its file. The library itself
    /// stays mapped until the process exits.
    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        let mut plugins = self.plugins.write();
        let index = removable_index(&plugins, id)?;
        let plugin = plugins.remove(index);
        let path = plugin.path.expect("external plugins have a file");
        fs::remove_file(&path).map_err(|e| format!("cannot delete {}: {e}", path.display()))
    }

    pub fn list(&self) -> Vec<PluginSummary> {
        self.plugins
            .read()
            .iter()
            .map(|p| PluginSummary {
                info: p.info.clone(),
                filename: p
                    .path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned()),
            })
            .collect()
    }

    pub fn count(&self) -> usize {
        self.plugins.read().len()
    }

    /// Factory for a maker type, with the id of the plugin providing it.
    pub fn maker_factory(&self, type_name: &str) -> Option<(String, Arc<dyn MakerFactory>)> {
        find_maker(&self.plugins.read(), type_name)
    }

    /// Factory for a sender type, with the id of the plugin providing it.
    pub fn sender_factory(&self, type_name: &str) -> Option<(String, Arc<dyn SenderFactory>)> {
        find_sender(&self.plugins.read(), type_name)
    }

    pub fn maker_types(&self) -> Vec<TypeInfo> {
        let plugins = self.plugins.read();
        plugins
            .iter()
            .flat_map(|p| &p.makers)
            .map(|f| TypeInfo {
                type_name: f.type_name().to_owned(),
                args: f.args(),
            })
            .collect()
    }

    pub fn sender_types(&self) -> Vec<TypeInfo> {
        let plugins = self.plugins.read();
        plugins
            .iter()
            .flat_map(|p| &p.senders)
            .map(|f| TypeInfo {
                type_name: f.type_name().to_owned(),
                args: f.args(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_uploaded_file_names() {
        assert_eq!(sanitize_file_name(Some("../../etc/lib evil$.so")), "lib_evil_.so");
        assert_eq!(sanitize_file_name(Some("C:\\x\\plugin.dll")), "plugin.dll");
        assert_eq!(
            sanitize_file_name(Some("  ")),
            format!("plugin.{}", std::env::consts::DLL_EXTENSION)
        );
        assert_eq!(
            sanitize_file_name(None),
            format!("plugin.{}", std::env::consts::DLL_EXTENSION)
        );
    }

    #[test]
    fn rejects_non_library_uploads_and_builtin_removal() {
        let dir = tempfile::tempdir().unwrap();
        let manager = PluginManager::new(dir.path().to_owned());
        manager.register(logmaker_default_plugin::definition()).unwrap();
        assert!(
            manager
                .install(Some("old-plugin.jar"), b"PK")
                .unwrap_err()
                .contains("unsupported file type")
        );
        assert!(manager.install(Some("broken.so"), b"not a library").is_err());
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            0,
            "failed uploads are deleted"
        );
        assert_eq!(
            manager.uninstall("default-plugin"),
            Err("built-in plugin cannot be deleted".into())
        );
        assert!(manager.register(logmaker_default_plugin::definition()).is_err());
    }
}
