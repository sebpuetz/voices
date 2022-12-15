fn main() -> anyhow::Result<()> {
    prost_build::Config::new()
        .out_dir("src")
        .compile_protos(&["../voice_proto/voice.proto"], &["../voice_proto"])?;
    Ok(())
}
