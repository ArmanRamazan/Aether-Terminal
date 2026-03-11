use std::path::Path;

fn main() {
    let dist = Path::new("frontend/dist");
    if !dist.exists() {
        println!("cargo:warning=frontend/dist not found — run `npm run build` in crates/aether-web/frontend/ first");
    }
    // Rebuild when dist/ contents change
    println!("cargo:rerun-if-changed=frontend/dist");
}
