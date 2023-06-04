fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .type_attribute("routeguide.Point", "#[derive(Hash)]")
        .compile(&["api/proto/transmit.proto"], &["proto"])?;

    Ok(())
}
