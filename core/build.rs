//! The web UI is embedded from `static/` (filled by `npm run build` in `ui/`).
//! Create the directory when the UI has not been built so the server still
//! compiles; it then answers UI routes with 404.

fn main() {
    let dir = std::path::Path::new("static");
    if !dir.exists() {
        std::fs::create_dir_all(dir).expect("create static/ directory");
    }
    println!("cargo:rerun-if-changed=static");
}
