// Embeds the Paprika icon and file info into the Windows .exe.
fn main() {
    println!("cargo:rerun-if-changed=assets/paprika.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/paprika.ico")
            .set("ProductName", "Paprika")
            .set("FileDescription", "Paprika Minecraft Launcher")
            .set("CompanyName", "Yeeboein")
            .set("LegalCopyright", "Copyright (c) Yeeboein");
        if let Err(e) = res.compile() {
            println!("cargo:warning=couldn't embed the Windows icon: {e}");
        }
    }
}
