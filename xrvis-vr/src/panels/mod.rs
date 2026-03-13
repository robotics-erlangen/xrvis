use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

pub mod game_state;

pub fn xr_panel_plugin(app: &mut App) {
    // Build a 1x1, -z forward, plane with mirrored uvs,
    // x-mirror because of the negative normal axis (-> "viewed from behind"),
    // y-mirror because y is down in UI coordinates
    let mesh_handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_indices(Indices::U16(vec![0, 1, 2, 1, 3, 2]))
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [-0.5, -0.5, 0.0],
                [-0.5, 0.5, 0.0],
                [0.5, -0.5, 0.0],
                [0.5, 0.5, 0.0],
            ],
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0., 0., -1.]; 4])
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_UV_0,
            vec![[1., 1.], [1., 0.], [0., 1.], [0., 0.]],
        ),
    );
    app.insert_resource(XrPanelMesh(mesh_handle));

    // 1000 res -> 1pixel=1mm, 10x scale -> 1unit=1cm
    app.insert_resource(UiScale(10.));
    app.insert_resource(XrPanelResolution {
        pixels_per_meter: 1000.,
    });
}

/// Marks the display mesh of an xr panel, and references the root of its UI hierarchy.
///
/// This separation is necessary because UI nodes can't have non-UI parents (or they won't be recognized as roots and won't be rendered),
/// but anchoring panels to other objects is a common usecase that requires the anchor as the parent.
#[derive(Component, Debug)]
#[relationship_target(relationship = XrUiRoot, linked_spawn)]
pub struct XrPanel(Entity);

/// Marks the root of a UI node that is rendering to the referenced panel.
///
/// This separation is necessary because UI nodes can't have non-UI parents (or they won't be recognized as roots and won't be rendered),
/// but anchoring panels to other objects is a common usecase that requires the anchor as the parent.
#[derive(Component, Debug)]
#[relationship(relationship_target = XrPanel)]
pub struct XrUiRoot(pub Entity);

/// Marker component to find entities used to anchor multiple panels.
#[derive(Component, Debug)]
pub struct XrPanelAnchor;

#[derive(Resource, Debug, Deref)]
struct XrPanelMesh(Handle<Mesh>);

#[derive(Resource, Clone, Copy, Debug, Deref)]
pub struct XrPanelResolution {
    pub pixels_per_meter: f32,
}

#[derive(SystemParam)]
pub struct XrPanelSpawner<'w> {
    panel_mesh: Res<'w, XrPanelMesh>,
    panel_res: Res<'w, XrPanelResolution>,
    image_assets: ResMut<'w, Assets<Image>>,
    material_assets: ResMut<'w, Assets<StandardMaterial>>,
}

impl XrPanelSpawner<'_> {
    /// Spawns a new spatial UI panel.
    ///
    /// The physical size of the panel is determined by the `x` and `y` components of the `transform` scale (in meters).
    /// The render resolution is calculated based on the physical size and the `XrPanelResolution` resource.
    ///
    /// The `ui_spawner` closure is used to build the UI hierarchy by spawning children under an
    /// automatically generated root node that covers the entire panel.
    pub fn spawn_panel(
        &mut self,
        commands: &mut Commands,
        transform: Transform,
        background_color: Color,
        ui_spawner: impl FnOnce(&mut RelatedSpawnerCommands<ChildOf>),
    ) -> Entity {
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
            unlit: true,
            alpha_mode: if background_color.is_fully_opaque() {
                AlphaMode::Opaque
            } else {
                AlphaMode::Mask(0.5) // Blending would require translucency sorting
            },
            ..default()
        });

        let mesh_handle = self.panel_mesh.clone();

        let ui_cam = commands
            .spawn((
                Camera2d,
                Camera {
                    // render before the "main pass" camera
                    order: -1,
                    clear_color: ClearColorConfig::Custom(background_color),
                    ..default()
                },
                RenderTarget::Image(image_handle.into()),
            ))
            .id();

        let ui_root = commands
            .spawn((
                Node {
                    width: percent(100),
                    height: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                UiTargetCamera(ui_cam),
            ))
            .add_child(ui_cam)
            .with_children(ui_spawner)
            .id();

        let display_panel = commands
            .spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                transform,
            ))
            .add_one_related::<XrUiRoot>(ui_root)
            .id();

        display_panel
    }
}
