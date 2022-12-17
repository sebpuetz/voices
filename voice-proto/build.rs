fn main() -> anyhow::Result<()> {
    let mut cfg = prost_build::Config::new();
    cfg.bytes(["."]);
    cfg.compile_protos(&["../voice_proto/voice.proto"], &["../voice_proto"])?;
    Ok(())
}
