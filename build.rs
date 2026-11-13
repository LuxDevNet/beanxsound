fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/beanxsound.rc");
        println!("cargo:rerun-if-changed=assets/beanxsound.manifest");
        println!("cargo:rerun-if-changed=assets/beanxsound.ico");
        let _ = embed_resource::compile("assets/beanxsound.rc", embed_resource::NONE)
            .manifest_optional();
    }
}
