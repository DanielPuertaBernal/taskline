//! Windows: embed the icon and the version information in taskline.exe.

fn main() {
    println!("cargo:rerun-if-changed=assets/taskline.ico");
    #[cfg(windows)]
    {
        let version = env!("CARGO_PKG_VERSION");
        let mut parts = version.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
        let packed = (parts.next().unwrap_or(0) << 48)
            | (parts.next().unwrap_or(0) << 32)
            | (parts.next().unwrap_or(0) << 16);
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/taskline.ico")
            .set("ProductName", "Taskline")
            .set("FileDescription", "Taskline")
            .set("CompanyName", "Daniel Puerta")
            .set("LegalCopyright", "© 2026 Daniel Puerta. MIT License.")
            .set("OriginalFilename", "taskline.exe")
            .set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed)
            .set_version_info(winresource::VersionInfo::FILEVERSION, packed);
        resource.compile().expect("Windows resources");
    }
}
