//! Embeds the application icon in the Windows executable. GPUI loads icon
//! resource 1 for the window, so the same icon shows in the title bar, the
//! taskbar and shortcuts.

fn main() {
    println!("cargo:rerun-if-changed=assets/castorterm.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/castorterm.ico");
        if let Err(err) = resource.compile() {
            // Never fail the build for the icon: warn and go on.
            println!("cargo:warning=could not embed the icon: {err}");
        }
    }
}
