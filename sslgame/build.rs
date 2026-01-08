use std::io::Result;

fn main() -> Result<()> {
    let proto_files =
        ["remote", "remote_meta", "remote_status"].map(|name| format!("src/proto/{}.proto", name));

    for path in &proto_files {
        println!("cargo:rerun-if-changed={}", path);
    }

    prost_build::compile_protos(&proto_files, &["src/proto/"])?;

    Ok(())
}
