//! JSON file persistence for maker, sender, log and scenario definitions.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Writes `value` as pretty JSON, atomically replacing `path`.
pub fn save_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> io::Result<()> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    fs::create_dir_all(dir)?;
    let mut file = tempfile::Builder::new()
        .prefix("logmaker-")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Reads JSON from `path`; a missing or blank file yields `T::default()`.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> io::Result<T> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(e),
    };
    if bytes.iter().all(u8::is_ascii_whitespace) {
        tracing::warn!("storage file {} is empty, starting with no entries", path.display());
        return Ok(T::default());
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", path.display())))
}

/// `name` of a stored entry (empty if missing).
pub fn entry_name(entry: &Value) -> &str {
    entry.get("name").and_then(Value::as_str).unwrap_or_default()
}

/// Stored entries that could not be recreated at startup, e.g. because their
/// plugin is not installed or an entry they reference failed to load. They are
/// written back on every save, so a temporary problem never erases
/// definitions; creating an entry with the same name replaces them.
#[derive(Default)]
pub struct Unloaded(Mutex<Vec<Value>>);

impl Unloaded {
    pub fn keep(&self, entry: Value) {
        self.0.lock().push(entry);
    }

    pub fn forget(&self, name: &str) {
        self.0.lock().retain(|entry| entry_name(entry) != name);
    }

    pub fn len(&self) -> usize {
        self.0.lock().len()
    }

    /// `stored` followed by the unloaded entries whose names it does not use.
    pub fn merge<T: Serialize>(&self, stored: &[T]) -> io::Result<Vec<Value>> {
        let mut entries = stored.iter().map(serde_json::to_value).collect::<Result<Vec<_>, _>>()?;
        let names: HashSet<String> = entries.iter().map(|e| entry_name(e).to_owned()).collect();
        let unloaded = self.0.lock();
        entries.extend(unloaded.iter().filter(|e| !names.contains(entry_name(e))).cloned());
        Ok(entries)
    }
}

/// Deserializes `null` as `T::default()` (the Java edition accepted nulls for
/// collections and primitives).
pub fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_blank_files_yield_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("none.json");
        assert_eq!(load_json::<Vec<String>>(&path).unwrap(), Vec::<String>::new());
        fs::write(&path, " \n").unwrap();
        assert_eq!(load_json::<Vec<String>>(&path).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn invalid_json_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        fs::write(&path, "{not json").unwrap();
        assert_eq!(
            load_json::<Vec<String>>(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn round_trips_and_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/data.json");
        save_json(&path, &vec!["a", "b"]).unwrap();
        assert_eq!(load_json::<Vec<String>>(&path).unwrap(), vec!["a", "b"]);
        let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "temporary files are cleaned up");
    }
}
