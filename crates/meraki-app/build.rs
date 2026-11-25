fn main() {
    println!("cargo:rerun-if-changed=../../packaging/windows/meraki.ico");
    #[cfg(windows)]
    embed_windows_resources();
}

#[cfg(windows)]
fn embed_windows_resources() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../packaging/windows/meraki.ico");
    res.set("ProductName", "Meraki");
    res.set("FileDescription", "Meraki Video Editor");

    if let Err(err) = res.compile() {
        eprintln!("cargo:warning=Falha ao embutir recursos do Windows: {err}");
        std::process::exit(1);
    }
}
