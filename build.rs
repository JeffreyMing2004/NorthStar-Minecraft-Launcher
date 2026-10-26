fn main() {
    println!("cargo:rerun-if-changed=assets/nscl.rc");
    println!("cargo:rerun-if-changed=assets/nscl.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/nscl.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to embed the NSCL application icon");
    }
}
