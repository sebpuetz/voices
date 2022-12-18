fn main() -> anyhow::Result<()> {
    prost_build::Config::new()
        .compile_protos(&["../voice_proto/voice.proto"], &["../voice_proto"])?;
    Ok(())
}
