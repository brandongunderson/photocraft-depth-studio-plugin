fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=authoring.json");
    let source =
        photocraft_plugin_sdk::api::AuthoringManifest::parse(&std::fs::read("authoring.json")?)?;
    let generated = photocraft_plugin_sdk::authoring::generate(&source)?;
    let output = std::path::PathBuf::from(
        std::env::var_os("OUT_DIR").ok_or("missing Cargo output directory")?,
    );
    generated.write_to(&output)?;
    Ok(())
}
