fn main() {
    embed_resource::compile("resource.rc", embed_resource::NONE).manifest_optional().unwrap();
}
