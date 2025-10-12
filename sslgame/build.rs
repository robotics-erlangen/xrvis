use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(
        &[
            "src/proto/status_streaming.proto",
            "src/proto/status_streaming_meta.proto",
        ],
        &["src/proto/"],
    )?;
    Ok(())
}
