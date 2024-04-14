fn main() -> anyhow::Result<()> {
    prost_build::Config::new().compile_protos(&["./voice.proto"], &["./"])?;
    Ok(())
}
