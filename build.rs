use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["src/sslgame/proto/status_compact.proto"], &["src/sslgame/proto/"])?;
    Ok(())
}
