fn main() {
    println!("cargo:rerun-if-changed=vendor/signalsmith");
    let mut cpp = cc::Build::new();
    cpp.file("vendor/signalsmith/src/wrapper.cpp")
        .include("vendor/signalsmith/signalsmith-stretch")
        .include("vendor/signalsmith")
        .cpp(true)
        .std("c++14")
        .opt_level(2)
        .warnings(false);
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Keep the C++ runtime in a DLL with its own matching CRT. Mixing the
        // LLVM/UCRT C++ archive into Rust's GCC/MSVCRT startup is not ABI-safe.
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let dll = out.join("amp-keylock.dll");
        let tool = cpp.get_compiler();
        let mut command = tool.to_command();
        if tool.is_like_msvc() {
            let exports = out.join("keylock.def");
            std::fs::write(&exports,"LIBRARY amp-keylock\nEXPORTS\nsignalsmith_stretch_create_preset_cheaper\nsignalsmith_stretch_destroy\nsignalsmith_stretch_set_transpose_factor\nsignalsmith_stretch_process\nsignalsmith_stretch_input_latency\nsignalsmith_stretch_output_latency\nsignalsmith_stretch_reset\n").unwrap();
            command
                .arg("/LD")
                .arg("/EHsc")
                .arg("/O2")
                .arg("/Ivendor/signalsmith")
                .arg("/Ivendor/signalsmith/signalsmith-stretch")
                .arg(format!("/Fo{}", out.join("keylock.obj").display()))
                .arg("vendor/signalsmith/src/wrapper.cpp")
                .arg("/link")
                .arg(format!("/DEF:{}", exports.display()))
                .arg(format!("/OUT:{}", dll.display()))
                .arg(format!("/IMPLIB:{}", out.join("keylock.lib").display()));
        } else {
            command
                .args([
                    "-shared",
                    "-static",
                    "-O2",
                    "-std=c++14",
                    "-Ivendor/signalsmith",
                    "-Ivendor/signalsmith/signalsmith-stretch",
                    "vendor/signalsmith/src/wrapper.cpp",
                    "-Wl,--export-all-symbols",
                    "-o",
                ])
                .arg(&dll);
        }
        assert!(
            command.status().expect("C++ key-lock compiler").success(),
            "Key Lock DLL compilation failed"
        );
        let profile = out
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .unwrap();
        std::fs::copy(&dll, profile.join("amp-keylock.dll")).unwrap();
        std::fs::create_dir_all(profile.join("deps")).unwrap();
        std::fs::copy(&dll, profile.join("deps/amp-keylock.dll")).unwrap();
    } else {
        cpp.compile("amp-signalsmith");
    }

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
