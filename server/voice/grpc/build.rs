fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=./service.proto");
    println!("cargo:rerun-if-changed=../../channels/grpc/channels.v1.proto");
    println!("cargo:rerun-if-changed=./build.rs");
    tonic_build::configure()
        .out_dir("src/registry")
        .build_server(false)
        .compile(
            &["../../channels/grpc/channels.v1.proto"],
            &["../../channels/grpc/"],
        )?;
    tonic_build::configure()
        .out_dir("src/")
        .build_client(false)
        .compile(&["./service.proto"], &["./"])?;
    Ok(())
}
