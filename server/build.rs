fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files = &["../proto/ortiofay.proto"];
    let dirs = &["../proto"];

    tonic_build::configure()
        .build_client(false)
        .build_server(true)
        .compile(files, dirs)?;

    // recompile protobufs only if any of the proto files changes.
    for file in files {
        println!("cargo:rerun-if-changed={}", file);
    }

    Ok(())
}
