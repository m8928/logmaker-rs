//! Loading external plugins built with `logmaker_plugin_api::export_plugin!`.
//!
//! Libraries are never unloaded: plugin code may own threads or thread-local
//! destructors, so `dlclose` while the process runs is unsound. Deleting a
//! plugin unregisters its types and removes its file only.

use std::path::Path;
use std::sync::Arc;

use logmaker_plugin_api::ffi::{
    ABI_VERSION, ENTRY_SYMBOL, EntryFn, FfiStr, FfiString, Handle, PluginDescriptor, PluginVTable, STATUS_OK,
    decode_error,
};
use logmaker_plugin_api::{ArgSpec, Args, Maker, MakerFactory, PluginError, Sender, SenderFactory};

use super::LoadedPlugin;

type CreateFn = unsafe extern "C" fn(FfiStr, FfiStr, FfiStr, *mut Handle, *mut FfiString) -> i32;

pub(super) fn load(path: &Path) -> Result<LoadedPlugin, String> {
    // SAFETY: loading a library runs its initialisers; uploading a plugin is a
    // trusted operation, like installing a JAR in the Java edition.
    let library = unsafe { libloading::Library::new(path) }.map_err(|e| format!("cannot load library: {e}"))?;
    // SAFETY: the symbol type matches `export_plugin!`.
    let entry = unsafe { library.get::<EntryFn>(ENTRY_SYMBOL) }
        .map(|symbol| *symbol)
        .map_err(|_| "not a LogMaker plugin (missing logmaker_plugin_v1 entry point)".to_owned())?;
    // Plugin code has run from here on, so the library must stay mapped.
    std::mem::forget(library);

    // SAFETY: the library stays loaded, so the entry point remains valid.
    let vtable = unsafe { entry() };
    if vtable.is_null() {
        return Err("plugin initialisation failed".into());
    }
    // SAFETY: the vtable is a static inside the never-unloaded library.
    let vtable: &'static PluginVTable = unsafe { &*vtable };
    if vtable.abi_version != ABI_VERSION {
        return Err(format!(
            "unsupported plugin ABI version {} (this server supports {ABI_VERSION})",
            vtable.abi_version
        ));
    }

    // SAFETY: `descriptor` takes no arguments.
    let descriptor = take_string(vtable, unsafe { (vtable.descriptor)() });
    let descriptor: PluginDescriptor =
        serde_json::from_str(&descriptor).map_err(|e| format!("invalid plugin descriptor: {e}"))?;
    if descriptor.info.id.trim().is_empty() {
        return Err("plugin descriptor has an empty id".into());
    }

    Ok(LoadedPlugin {
        info: descriptor.info,
        path: Some(path.to_owned()),
        makers: descriptor
            .makers
            .into_iter()
            .map(|t| -> Arc<dyn MakerFactory> {
                Arc::new(NativeMakerFactory {
                    vtable,
                    type_name: t.type_name,
                    args: t.args,
                })
            })
            .collect(),
        senders: descriptor
            .senders
            .into_iter()
            .map(|t| -> Arc<dyn SenderFactory> {
                Arc::new(NativeSenderFactory {
                    vtable,
                    type_name: t.type_name,
                    args: t.args,
                })
            })
            .collect(),
    })
}

/// Copies a plugin-allocated string and releases it.
fn take_string(vtable: &PluginVTable, s: FfiString) -> String {
    // SAFETY: `s` was just returned by the plugin and is freed exactly once below.
    let owned = unsafe { s.to_owned_string() };
    if !s.ptr.is_null() {
        // SAFETY: `s` came from this plugin and is not used afterwards.
        unsafe { (vtable.free_string)(s) };
    }
    owned
}

fn create(
    vtable: &PluginVTable,
    create: CreateFn,
    type_name: &str,
    name: &str,
    args: &Args,
) -> Result<Handle, PluginError> {
    let args = serde_json::Value::Object(args.clone()).to_string();
    let mut handle: Handle = std::ptr::null_mut();
    let mut err = FfiString::empty();
    // SAFETY: the strings outlive the call and the out-pointers are valid.
    let status = unsafe {
        create(
            FfiStr::new(type_name),
            FfiStr::new(name),
            FfiStr::new(&args),
            &mut handle,
            &mut err,
        )
    };
    let message = take_string(vtable, err);
    if status == STATUS_OK && !handle.is_null() {
        Ok(handle)
    } else {
        Err(decode_error(status, message))
    }
}

struct NativeMakerFactory {
    vtable: &'static PluginVTable,
    type_name: String,
    args: Vec<ArgSpec>,
}

impl MakerFactory for NativeMakerFactory {
    fn type_name(&self) -> &str {
        &self.type_name
    }

    fn args(&self) -> Vec<ArgSpec> {
        self.args.clone()
    }

    fn create(&self, name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        let handle = create(self.vtable, self.vtable.maker_create, &self.type_name, name, args)?;
        Ok(Box::new(NativeMaker {
            vtable: self.vtable,
            handle,
        }))
    }
}

struct NativeMaker {
    vtable: &'static PluginVTable,
    handle: Handle,
}

// SAFETY: the handle points to a `Box<dyn Maker>` inside the plugin, and
// `Maker: Send + Sync`.
unsafe impl Send for NativeMaker {}
// SAFETY: see above.
unsafe impl Sync for NativeMaker {}

impl Maker for NativeMaker {
    fn get_data(&self) -> String {
        // SAFETY: `handle` is live until `drop`.
        take_string(self.vtable, unsafe { (self.vtable.maker_get_data)(self.handle) })
    }

    fn size(&self) -> u64 {
        // SAFETY: `handle` is live until `drop`.
        unsafe { (self.vtable.maker_size)(self.handle) }
    }
}

impl Drop for NativeMaker {
    fn drop(&mut self) {
        // SAFETY: the handle is destroyed exactly once.
        unsafe { (self.vtable.maker_destroy)(self.handle) };
    }
}

struct NativeSenderFactory {
    vtable: &'static PluginVTable,
    type_name: String,
    args: Vec<ArgSpec>,
}

impl SenderFactory for NativeSenderFactory {
    fn type_name(&self) -> &str {
        &self.type_name
    }

    fn args(&self) -> Vec<ArgSpec> {
        self.args.clone()
    }

    fn create(&self, name: &str, args: &Args) -> Result<Box<dyn Sender>, PluginError> {
        let handle = create(self.vtable, self.vtable.sender_create, &self.type_name, name, args)?;
        Ok(Box::new(NativeSender {
            vtable: self.vtable,
            handle,
        }))
    }
}

struct NativeSender {
    vtable: &'static PluginVTable,
    handle: Handle,
}

// SAFETY: the handle points to a `Box<dyn Sender>` inside the plugin, and
// `Sender: Send + Sync`.
unsafe impl Send for NativeSender {}
// SAFETY: see above.
unsafe impl Sync for NativeSender {}

impl Sender for NativeSender {
    fn send(&self, data: &str) -> Result<(), PluginError> {
        let mut err = FfiString::empty();
        // SAFETY: `handle` is live until `drop`; `data` outlives the call.
        let status = unsafe { (self.vtable.sender_send)(self.handle, FfiStr::new(data), &mut err) };
        let message = take_string(self.vtable, err);
        if status == STATUS_OK {
            Ok(())
        } else {
            Err(decode_error(status, message))
        }
    }
}

impl Drop for NativeSender {
    fn drop(&mut self) {
        // SAFETY: the handle is destroyed exactly once.
        unsafe { (self.vtable.sender_destroy)(self.handle) };
    }
}
