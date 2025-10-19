// The package has to be available as a library for android builds, so this is just a simple wrapper to keep desktop compatibility.
fn main() -> bevy::app::AppExit {
    xrvis_vr_lib::main()
}
