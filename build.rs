fn main() {
    println!("cargo:rerun-if-changed=native/macos/CFtopSys.c");
    println!("cargo:rerun-if-changed=native/macos/CFtopSys.h");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/macos/CFtopSys.c")
            .include("native/macos")
            .compile("ftop_sensors");
        println!("cargo:rustc-link-lib=framework=IOKit");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=IOReport");
    }
}
