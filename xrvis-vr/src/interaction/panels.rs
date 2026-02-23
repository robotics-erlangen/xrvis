use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::color::palettes::basic::{BLUE, GRAY, RED};
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use std::f32::consts::PI;

pub fn xr_panel_plugin(app: &mut App) {
    let mesh_handle = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Plane3d::new(Vec3::NEG_Z, Vec2::splat(0.5)));
    app.insert_resource(XrPanelMesh(mesh_handle));

    // 1000 res -> 1pixel=1mm, 10x scale -> 1unit=1cm
    app.insert_resource(UiScale(10.));
    app.insert_resource(XrPanelResolution {
        pixels_per_meter: 1000.,
    });

    let test_panel = |parent: &mut RelatedSpawnerCommands<ChildOf>| {
        parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: auto(),
                    height: auto(),
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(10.)),
                    border_radius: BorderRadius::all(px(10.)),
                    ..default()
                },
                BackgroundColor(BLUE.into()),
            ))
            .observe(
                |drag: On<Pointer<Drag>>, mut nodes: Query<(&mut Node, &ComputedNode)>| {
                    let (mut node, computed) = nodes.get_mut(drag.entity).unwrap();
                    node.left = px(drag.pointer_location.position.x - computed.size.x / 2.0) / 10.;
                    node.top = px(drag.pointer_location.position.y - computed.size.y / 2.0) / 10.;
                },
            )
            .observe(
                |over: On<Pointer<Over>>, mut colors: Query<&mut BackgroundColor>| {
                    colors.get_mut(over.entity).unwrap().0 = RED.into();
                },
            )
            .observe(
                |out: On<Pointer<Out>>, mut colors: Query<&mut BackgroundColor>| {
                    colors.get_mut(out.entity).unwrap().0 = BLUE.into();
                },
            )
            .with_children(|parent| {
                parent.spawn((
                    Text::new("Drag Me!"),
                    TextFont {
                        font_size: 5.,
                        ..default()
                    },
                    TextColor::WHITE,
                ));
            });
    };

    app.add_systems(Startup, move |mut panel_spawner: XrPanelSpawner| {
        panel_spawner.spawn_panel(
            Transform {
                translation: Vec3::new(0., 1., 0.),
                rotation: Quat::from_rotation_x(PI / 4.),
                scale: Vec3::new(1., 1., 1.),
            },
            Color::srgba(0.5, 1.0, 0.5, 0.8),
            test_panel,
        );
        panel_spawner.spawn_panel(
            Transform {
                translation: Vec3::new(0., 0.5, 0.),
                rotation: Quat::from_rotation_x(PI / 4.),
                scale: Vec3::new(2., 1., 1.),
            },
            GRAY.into(),
            test_panel,
        );
    });
}

#[derive(Component, Debug)]
pub struct XrPanel;

#[derive(Resource, Debug, Deref)]
struct XrPanelMesh(Handle<Mesh>);

#[derive(Resource, Clone, Copy, Debug, Deref)]
pub struct XrPanelResolution {
    pub pixels_per_meter: f32,
}

#[derive(SystemParam)]
pub struct XrPanelSpawner<'w, 's> {
    commands: Commands<'w, 's>,
    panel_mesh: Res<'w, XrPanelMesh>,
    panel_res: Res<'w, XrPanelResolution>,
    image_assets: ResMut<'w, Assets<Image>>,
    material_assets: ResMut<'w, Assets<StandardMaterial>>,
}

impl XrPanelSpawner<'_, '_> {
    /// Spawns a new spatial UI panel.
    ///
    /// The physical size of the panel is determined by the `x` and `y` components of the `transform` scale (in meters).
    /// The render resolution is calculated based on the physical size and the `XrPanelResolution` resource.
    ///
    /// The `ui_spawner` closure is used to build the UI hierarchy by spawning children under an
    /// automatically generated root node that covers the entire panel.
    pub fn spawn_panel(
        &mut self,
        transform: Transform,
        background_color: Color,
        ui_spawner: impl FnOnce(&mut RelatedSpawnerCommands<ChildOf>),
    ) {
        let mut image = Image::new_fill(
            Extent3d {
                width: (transform.scale.x * self.panel_res.pixels_per_meter) as u32,
                height: (transform.scale.y * self.panel_res.pixels_per_meter) as u32,
                ..default()
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Bgra8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT;

        let image_handle = self.image_assets.add(image);

        let material_handle = self.material_assets.add(StandardMaterial {
            base_color_texture: Some(image_handle.clone()),
            reflectance: 0.02,
            unlit: false,
            alpha_mode: if background_color.is_fully_opaque() {
                AlphaMode::Opaque
            } else {
                AlphaMode::Blend
            },
            ..default()
        });

        let mesh_handle = self.panel_mesh.clone();

        let ui_cam = self
            .commands
            .spawn((
                Camera2d,
                Camera {
                    // render before the "main pass" camera
                    order: -1,
                    ..default()
                },
                RenderTarget::Image(image_handle.into()),
            ))
            .id();

        self.commands
            .spawn((
                // Panel
                XrPanel,
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                transform,
                // UI Root
                Node {
                    width: percent(100),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(background_color),
                UiTargetCamera(ui_cam),
            ))
            .with_children(ui_spawner);
    }

    // TODO: Handle despawning. Maybe using relations?
}
