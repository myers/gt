use std::{
    env,
    fs::{self, File},
    path::Path,
};

fn main() {
    let src = "openapi.v1.json";
    println!("cargo:rerun-if-changed={src}");

    let file = File::open(src).unwrap();
    let spec = serde_json::from_reader(file).unwrap();

    let mut settings = progenitor::GenerationSettings::new();
    settings
        .with_interface(progenitor::InterfaceStyle::Builder)
        .with_tag(progenitor::TagStyle::Merged);

    let mut generator = progenitor::Generator::new(&settings);
    let tokens = generator.generate_tokens(&spec).unwrap();
    let ast = syn::parse2(tokens).unwrap();
    let mut content = prettyplease::unparse(&ast);

    // Gitea may send `null` for empty arrays. Inject null-safe deserializer.
    content = content.replace(
        r#"#[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]"#,
        r#"#[serde(default, deserialize_with = "crate::null_as_default", skip_serializing_if = "::std::vec::Vec::is_empty")]"#,
    );

    let out_file = Path::new(&env::var("OUT_DIR").unwrap()).join("codegen.rs");
    fs::write(out_file, content).unwrap();
}
