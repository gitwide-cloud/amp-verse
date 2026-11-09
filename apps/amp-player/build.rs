fn main() {
    println!("cargo:rerun-if-changed=../../assets/amp-verse.res");
    println!("cargo:rerun-if-changed=../../assets/amp-verse.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let resource = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/amp-verse.res")
            .canonicalize()
            .expect("Bundled Windows icon resource");
        println!("cargo:rustc-link-arg-bin=amp-player={}", resource.display());
    }
}
