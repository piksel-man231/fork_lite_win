fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest::embed_manifest_file("Assets/app.manifest").expect("Windows manifest");
        embed_resource::compile("Assets/app.rc", embed_resource::NONE)
            .manifest_required()
            .expect("Windows icon");
    }
}
