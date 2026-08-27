fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico")
            .set_manifest_file("photon_count_adjuster.exe.manifest")
            .compile()
            .expect("failed to compile Windows resources");
    }
}
