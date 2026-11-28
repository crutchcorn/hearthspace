// Naga's build dependency below also unifies its diagnostic feature.
fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_UDEV");
    if std::env::var_os("CARGO_FEATURE_UDEV").is_none() {
        return;
    }
    let library = pkg_config::Config::new()
        .atleast_version("0.1.0")
        .probe("libxcvt")
        .expect("native display modes require libxcvt-dev; see docs/DEPENDENCIES.md");
    let mut builder = bindgen::Builder::default()
        .header_contents(
            "xcvt-wrapper.h",
            "#include <stdlib.h>\n#include <libxcvt/libxcvt.h>",
        )
        .allowlist_function("libxcvt_gen_mode_info|free")
        .allowlist_type("libxcvt_mode_info|libxcvt_mode_flags")
        .layout_tests(false);
    for include in library.include_paths {
        builder = builder.clang_arg(format!("-I{}", include.display()));
    }
    // The workspace-pinned bindgen predates Rust 2024's unsafe-extern syntax.
    // Adapt only the generated declarations rather than adding a second version.
    let bindings = builder
        .generate()
        .expect("libxcvt bindings")
        .to_string()
        .replace("extern \"C\" {", "unsafe extern \"C\" {");
    std::fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("xcvt.rs"),
        bindings,
    )
    .expect("write libxcvt bindings");
}
