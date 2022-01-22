fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico")
            .set_manifest_file("photon_count_adjuster.exe.manifest")
            .compile()
            .unwrap();
    }
}
