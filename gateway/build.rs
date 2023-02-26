#[cfg(feature = "distributed")]
fn main() -> anyhow::Result<()> {
    use anyhow::Context;

    println!("cargo:rerun-if-changed=../channels-internal/channels.v1.proto");
    println!("cargo:rerun-if-changed=./build.rs");

    tonic_build::configure()
        .out_dir("src/server/channels")
        .build_server(false)
        .compile(
            &["../channels-internal/channels.v1.proto"],
            &["../channels-internal/"],
        )
        .context("failed to compile channel registry proto")?;

    println!("cargo:rerun-if-changed=../voice-server/service.proto");

    tonic_build::configure()
        .out_dir("src/server/voice_instance")
        .build_server(false)
        .compile(&["../voice-server/service.proto"], &["../voice-server/"])
        .context("failed to compile voice server proto")?;
    Ok(())
}

#[cfg(not(feature = "distributed"))]
fn main() {}
