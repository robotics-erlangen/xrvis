use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["src/sslgame/proto/status_streaming.proto", "src/sslgame/proto/status_streaming_meta.proto"], &["src/sslgame/proto/"])?;
    Ok(())
}
