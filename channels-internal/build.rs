fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=./service.proto");
    println!("cargo:rerun-if-changed=./build.rs");
    println!("cargo:rerun-if-changed=./migrations/");
    tonic_build::configure()
        .out_dir("src/grpc")
        .compile(&["./service.proto"], &["./"])?;
    Ok(())
}
