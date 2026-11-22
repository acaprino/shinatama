//! Embeds the program's icon in `shinatama.exe`: Imago's mark for Shinatama, the same as the window program's.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=shinatama.rc");
    println!("cargo:rerun-if-changed=icon.ico");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("shinatama.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
