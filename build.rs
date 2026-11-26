// Embeds the app icon (build/icon.png → .ico) and version info into cc-launcher.exe.
fn main() {
    println!("cargo:rerun-if-changed=build/icon.png");
    #[cfg(windows)]
    {
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("icon.ico");
        let img = image::open("build/icon.png").expect("build/icon.png");
        img.resize(256, 256, image::imageops::FilterType::Lanczos3).save(&out).expect("write icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon(out.to_str().unwrap());
        res.set("ProductName", "cc-launcher");
        res.set("FileDescription", "cc-launcher");
        res.compile().expect("embed resources");
    }
}
