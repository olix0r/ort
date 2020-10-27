fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files = &["src/proto/strest.proto"];
    let dirs = &["src/proto"];

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile(files, dirs)?;

    // recompile protobufs only if any of the proto files changes.
    for file in files {
        println!("cargo:rerun-if-changed={}", file);
    }

    Ok(())
}
