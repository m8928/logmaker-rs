//! Stable C ABI between the LogMaker server and external plugin libraries.
//!
//! A plugin library exports one symbol, [`ENTRY_SYMBOL`], returning a pointer to
//! a static [`PluginVTable`]. Every value crosses the boundary as UTF-8 bytes
//! (JSON for structured data), so the server and the plugin may be built with
//! different Rust compilers. Strings allocated by the plugin are returned as
//! [`FfiString`] and must be released with [`PluginVTable::free_string`].
//!
//! Plugin authors do not use this module directly; [`export_plugin!`] generates
//! the export from a [`PluginDefinition`].
//!
//! [`export_plugin!`]: crate::export_plugin

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::{ArgSpec, Args, Maker, PluginDefinition, PluginError, PluginInfo, Sender};

/// Incremented on every incompatible change to [`PluginVTable`].
pub const ABI_VERSION: u32 = 1;

/// Name of the entry point exported by plugin libraries.
pub const ENTRY_SYMBOL: &[u8] = b"logmaker_plugin_v1";

/// Signature of the entry point.
pub type EntryFn = unsafe extern "C" fn() -> *const PluginVTable;

/// Opaque maker/sender instance owned by the plugin.
pub type Handle = *mut c_void;

/// Status code returned by `*_create` and `sender_send`.
pub const STATUS_OK: i32 = 0;
/// The `err` out-parameter holds the invalid argument name (may be empty).
pub const STATUS_INVALID_ARGUMENT: i32 = 1;
/// The `err` out-parameter holds an error message.
pub const STATUS_FAILED: i32 = 2;

/// Borrowed UTF-8 string passed into the plugin.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiStr {
    pub ptr: *const u8,
    pub len: usize,
}

impl FfiStr {
    pub fn new(s: &str) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }

    /// # Safety
    /// `ptr`/`len` must describe a live byte slice.
    pub unsafe fn as_str<'a>(self) -> &'a str {
        if self.ptr.is_null() || self.len == 0 {
            return "";
        }
        // SAFETY: guaranteed by the caller.
        let bytes = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
        std::str::from_utf8(bytes).unwrap_or("")
    }
}

/// UTF-8 string allocated by the plugin; release it with
/// [`PluginVTable::free_string`].
#[repr(C)]
pub struct FfiString {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}

impl FfiString {
    pub fn empty() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
            cap: 0,
        }
    }

    pub fn from_string(s: String) -> Self {
        let mut bytes = std::mem::ManuallyDrop::new(s.into_bytes());
        Self {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
            cap: bytes.capacity(),
        }
    }

    /// Copies the content into a host-owned `String`.
    ///
    /// # Safety
    /// Must describe a string produced by [`FfiString::from_string`] that has not
    /// been freed yet.
    pub unsafe fn to_owned_string(&self) -> String {
        if self.ptr.is_null() || self.len == 0 {
            return String::new();
        }
        // SAFETY: guaranteed by the caller.
        let bytes = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Function table exported by a plugin library.
///
/// # Safety
/// Callers must pass handles returned by the matching `*_create` function and
/// not yet destroyed, strings that are valid UTF-8 for the duration of the
/// call, valid out-pointers, and only [`FfiString`]s returned by this table to
/// `free_string` (each exactly once).
#[repr(C)]
pub struct PluginVTable {
    pub abi_version: u32,
    /// Returns the [`PluginDescriptor`] as JSON.
    pub descriptor: unsafe extern "C" fn() -> FfiString,
    pub free_string: unsafe extern "C" fn(FfiString),

    /// Creates a maker: `(type_name, name, args_json, out_handle, out_err) -> status`.
    pub maker_create: unsafe extern "C" fn(FfiStr, FfiStr, FfiStr, *mut Handle, *mut FfiString) -> i32,
    pub maker_get_data: unsafe extern "C" fn(Handle) -> FfiString,
    pub maker_size: unsafe extern "C" fn(Handle) -> u64,
    pub maker_destroy: unsafe extern "C" fn(Handle),

    /// Creates a sender: `(type_name, name, args_json, out_handle, out_err) -> status`.
    pub sender_create: unsafe extern "C" fn(FfiStr, FfiStr, FfiStr, *mut Handle, *mut FfiString) -> i32,
    /// Sends one line: `(handle, data, out_err) -> status`.
    pub sender_send: unsafe extern "C" fn(Handle, FfiStr, *mut FfiString) -> i32,
    pub sender_destroy: unsafe extern "C" fn(Handle),
}

/// Plugin metadata and the types it provides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDescriptor {
    #[serde(flatten)]
    pub info: PluginInfo,
    pub makers: Vec<TypeDescriptor>,
    pub senders: Vec<TypeDescriptor>,
}

/// One maker or sender type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDescriptor {
    #[serde(rename = "type")]
    pub type_name: String,
    pub args: Vec<ArgSpec>,
}

/// Encodes an error into an out-parameter and returns its status code.
fn write_error(err: *mut FfiString, error: PluginError) -> i32 {
    let (status, text) = match error {
        PluginError::InvalidArgument(name) => (STATUS_INVALID_ARGUMENT, name.unwrap_or_default()),
        PluginError::Failed(message) => (STATUS_FAILED, message),
    };
    if !err.is_null() {
        // SAFETY: the host passes a valid, writable out-parameter.
        unsafe { err.write(FfiString::from_string(text)) };
    }
    status
}

/// Decodes a status code and error text produced by the plugin.
pub fn decode_error(status: i32, text: String) -> PluginError {
    match status {
        STATUS_INVALID_ARGUMENT => PluginError::InvalidArgument((!text.is_empty()).then_some(text)),
        _ => PluginError::Failed(if text.is_empty() {
            "plugin call failed".into()
        } else {
            text
        }),
    }
}

// ---------------------------------------------------------------------------
// Plugin side: implementation behind `export_plugin!`
// ---------------------------------------------------------------------------

static EXPORTED: OnceLock<PluginDefinition> = OnceLock::new();

static VTABLE: PluginVTable = PluginVTable {
    abi_version: ABI_VERSION,
    descriptor: export_descriptor,
    free_string: export_free_string,
    maker_create: export_maker_create,
    maker_get_data: export_maker_get_data,
    maker_size: export_maker_size,
    maker_destroy: export_maker_destroy,
    sender_create: export_sender_create,
    sender_send: export_sender_send,
    sender_destroy: export_sender_destroy,
};

/// Called by the generated entry point. Returns null if building the
/// definition panics.
#[doc(hidden)]
pub fn export(definition: fn() -> PluginDefinition) -> *const PluginVTable {
    match catch_unwind(|| {
        EXPORTED.get_or_init(definition);
    }) {
        Ok(()) => &VTABLE,
        Err(_) => std::ptr::null(),
    }
}

fn exported() -> &'static PluginDefinition {
    EXPORTED
        .get()
        .expect("plugin definition is initialised by the entry point")
}

fn parse_args(json: &str) -> Result<Args, PluginError> {
    serde_json::from_str(json).map_err(|e| PluginError::failed(format!("invalid arguments JSON: {e}")))
}

fn guarded<T>(fallback: impl FnOnce() -> T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| fallback())
}

fn panicked() -> PluginError {
    PluginError::failed("plugin panicked")
}

extern "C" fn export_descriptor() -> FfiString {
    guarded(FfiString::empty, || {
        let definition = exported();
        let descriptor = PluginDescriptor {
            info: definition.info.clone(),
            makers: definition
                .makers
                .iter()
                .map(|f| TypeDescriptor {
                    type_name: f.type_name().to_owned(),
                    args: f.args(),
                })
                .collect(),
            senders: definition
                .senders
                .iter()
                .map(|f| TypeDescriptor {
                    type_name: f.type_name().to_owned(),
                    args: f.args(),
                })
                .collect(),
        };
        FfiString::from_string(serde_json::to_string(&descriptor).unwrap_or_default())
    })
}

extern "C" fn export_free_string(s: FfiString) {
    if !s.ptr.is_null() {
        // SAFETY: `s` was produced by `FfiString::from_string` in this library.
        drop(unsafe { Vec::from_raw_parts(s.ptr, s.len, s.cap) });
    }
}

extern "C" fn export_maker_create(
    type_name: FfiStr,
    name: FfiStr,
    args_json: FfiStr,
    out: *mut Handle,
    err: *mut FfiString,
) -> i32 {
    let result = guarded(
        || Err(panicked()),
        || {
            // SAFETY: the host passes live strings for the duration of the call.
            let (type_name, name, args_json) = unsafe { (type_name.as_str(), name.as_str(), args_json.as_str()) };
            let factory = exported()
                .makers
                .iter()
                .find(|f| f.type_name() == type_name)
                .ok_or_else(|| PluginError::failed(format!("unknown maker type {type_name}")))?;
            factory.create(name, &parse_args(args_json)?)
        },
    );
    match result {
        Ok(maker) => {
            // SAFETY: the host passes a valid, writable out-parameter.
            unsafe { out.write(Box::into_raw(Box::new(maker)).cast()) };
            STATUS_OK
        }
        Err(e) => write_error(err, e),
    }
}

/// # Safety
/// `handle` must come from `export_maker_create` and not be destroyed.
unsafe fn maker_ref<'a>(handle: Handle) -> &'a dyn Maker {
    // SAFETY: guaranteed by the caller.
    unsafe { &**handle.cast::<Box<dyn Maker>>() }
}

extern "C" fn export_maker_get_data(handle: Handle) -> FfiString {
    // SAFETY: the host only passes live handles.
    guarded(FfiString::empty, || {
        FfiString::from_string(unsafe { maker_ref(handle) }.get_data())
    })
}

extern "C" fn export_maker_size(handle: Handle) -> u64 {
    // SAFETY: the host only passes live handles.
    guarded(|| 0, || unsafe { maker_ref(handle) }.size())
}

extern "C" fn export_maker_destroy(handle: Handle) {
    if !handle.is_null() {
        // SAFETY: the host destroys each handle exactly once.
        guarded(
            || (),
            || drop(unsafe { Box::from_raw(handle.cast::<Box<dyn Maker>>()) }),
        );
    }
}

extern "C" fn export_sender_create(
    type_name: FfiStr,
    name: FfiStr,
    args_json: FfiStr,
    out: *mut Handle,
    err: *mut FfiString,
) -> i32 {
    let result = guarded(
        || Err(panicked()),
        || {
            // SAFETY: the host passes live strings for the duration of the call.
            let (type_name, name, args_json) = unsafe { (type_name.as_str(), name.as_str(), args_json.as_str()) };
            let factory = exported()
                .senders
                .iter()
                .find(|f| f.type_name() == type_name)
                .ok_or_else(|| PluginError::failed(format!("unknown sender type {type_name}")))?;
            factory.create(name, &parse_args(args_json)?)
        },
    );
    match result {
        Ok(sender) => {
            // SAFETY: the host passes a valid, writable out-parameter.
            unsafe { out.write(Box::into_raw(Box::new(sender)).cast()) };
            STATUS_OK
        }
        Err(e) => write_error(err, e),
    }
}

extern "C" fn export_sender_send(handle: Handle, data: FfiStr, err: *mut FfiString) -> i32 {
    let result = guarded(
        || Err(panicked()),
        || {
            // SAFETY: the host only passes live handles and strings.
            let sender = unsafe { &**handle.cast::<Box<dyn Sender>>() };
            sender.send(unsafe { data.as_str() })
        },
    );
    match result {
        Ok(()) => STATUS_OK,
        Err(e) => write_error(err, e),
    }
}

extern "C" fn export_sender_destroy(handle: Handle) {
    if !handle.is_null() {
        // SAFETY: the host destroys each handle exactly once.
        guarded(
            || (),
            || drop(unsafe { Box::from_raw(handle.cast::<Box<dyn Sender>>()) }),
        );
    }
}

/// Exports a plugin library entry point.
///
/// Takes the path of a function returning the [`PluginDefinition`]:
///
/// ```ignore
/// fn definition() -> logmaker_plugin_api::PluginDefinition { /* ... */ }
///
/// logmaker_plugin_api::export_plugin!(definition);
/// ```
///
/// The crate must be built with `crate-type = ["cdylib"]`.
///
/// [`PluginDefinition`]: crate::PluginDefinition
#[macro_export]
macro_rules! export_plugin {
    ($definition:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn logmaker_plugin_v1() -> *const $crate::ffi::PluginVTable {
            $crate::ffi::export($definition)
        }
    };
}
