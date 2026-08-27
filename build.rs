fn main() {
    let dist = std::path::Path::new("web/dist");
    if let Err(err) = std::fs::create_dir_all(dist) {
        panic!("failed to create {}: {err}", dist.display());
    }
    println!("cargo:rerun-if-changed=web/dist");
}
