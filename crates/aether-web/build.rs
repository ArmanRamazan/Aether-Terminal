use std::path::Path;

fn main() {
    let dist = Path::new("frontend/dist");
    if !dist.exists() {
        println!("cargo:warning=frontend/dist not found — run `npm run build` in crates/aether-web/frontend/ first");
        // Create empty dist so RustEmbed derive generates a valid (empty) impl
        std::fs::create_dir_all(dist).ok();
    }
    // Rebuild when dist/ contents change
    println!("cargo:rerun-if-changed=frontend/dist");
}
