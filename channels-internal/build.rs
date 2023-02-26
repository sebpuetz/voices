fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=./channels.v1.proto");
    println!("cargo:rerun-if-changed=./build.rs");
    println!("cargo:rerun-if-changed=./migrations/");
    tonic_build::configure()
        .out_dir("src/grpc")
        .build_client(false)
        .compile(&["./channels.v1.proto"], &["./"])?;
    Ok(())
}
