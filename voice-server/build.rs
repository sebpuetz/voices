fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=./service.proto");
    println!("cargo:rerun-if-changed=../channels-internal/channels.v1.proto");
    println!("cargo:rerun-if-changed=./build.rs");
    tonic_build::configure()
        .out_dir("src/grpc")
        .build_client(false)
        .compile(&["./service.proto"], &["./"])?;
    tonic_build::configure()
        .out_dir("src/registry")
        .build_server(false)
        .compile(
            &["../channels-internal/channels.v1.proto"],
            &["../channels-internal/"],
        )?;
    Ok(())
}
