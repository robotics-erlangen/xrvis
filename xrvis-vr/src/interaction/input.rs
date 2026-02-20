use bevy::prelude::*;
use schminput::prelude::*;

#[derive(Resource, Clone, Copy, Debug)]
pub struct PointerActions {
    pub left_aim_pose: Entity,
    pub right_aim_pose: Entity,
    pub left_aim_activate: Entity,
    pub right_aim_activate: Entity,
}

#[derive(Component, Clone, Copy)]
#[require(Transform)]
pub struct LeftHandPointer;
#[derive(Component, Clone, Copy)]
#[require(Transform)]
pub struct RightHandPointer;

pub fn xr_input_plugin(app: &mut App) {
    app.add_plugins(DefaultSchminputPlugins);

    app.add_systems(Startup, setup_oxr_schminput);
}

fn setup_oxr_schminput(mut commands: Commands) {
    const HAND_PROFILE: &str = "/interaction_profiles/ext/hand_interaction_ext";
    // TODO: Use the touch plus profile after switching to openxr 1.1 (requires an extension and a different path name in 1.0)
    //const META_TOUCH_PLUS_PROFILE: &str = "/interaction_profiles/meta/touch_plus_controller";

    // ======== Pointer actions ========

    let pointer_set = commands
        .spawn(ActionSet::new("pointers", "Pointers", 0))
        .id();

    let left_pointer = commands.spawn(LeftHandPointer).id();
    let right_pointer = commands.spawn(RightHandPointer).id();
    let left_aim_pose = commands
        .spawn((
            Action::new("left_aim_pose", "Left Hand Pointer", pointer_set),
            OxrBindings::new()
                .bindings(HAND_PROFILE, ["/user/hand/left/input/aim/pose"])
                .bindings(OCULUS_TOUCH_PROFILE, ["/user/hand/left/input/aim/pose"]),
            SpaceActionValue::new(),
            AttachSpaceToEntity(left_pointer),
        ))
        .id();
    let right_aim_pose = commands
        .spawn((
            Action::new("right_aim_pose", "Right Hand Pointer", pointer_set),
            OxrBindings::new()
                .bindings(HAND_PROFILE, ["/user/hand/right/input/aim/pose"])
                .bindings(OCULUS_TOUCH_PROFILE, ["/user/hand/right/input/aim/pose"]),
            SpaceActionValue::new(),
            AttachSpaceToEntity(right_pointer),
        ))
        .id();
    let left_aim_activate = commands
        .spawn((
            Action::new("left_aim_activate", "Left Hand Pointer Click", pointer_set),
            OxrBindings::new()
                .bindings(
                    HAND_PROFILE,
                    ["/user/hand/left/input/aim_activate_ext/value"],
                )
                .bindings(
                    OCULUS_TOUCH_PROFILE,
                    ["/user/hand/left/input/trigger/value"],
                ),
            F32ActionValue::new(),
        ))
        .id();
    let right_aim_activate = commands
        .spawn((
            Action::new(
                "right_aim_activate",
                "Right Hand Pointer Click",
                pointer_set,
            ),
            OxrBindings::new()
                .bindings(
                    HAND_PROFILE,
                    ["/user/hand/right/input/aim_activate_ext/value"],
                )
                .bindings(
                    OCULUS_TOUCH_PROFILE,
                    ["/user/hand/right/input/trigger/value"],
                ),
            F32ActionValue::new(),
        ))
        .id();

    commands.insert_resource(PointerActions {
        left_aim_pose,
        right_aim_pose,
        left_aim_activate,
        right_aim_activate,
    });
}
