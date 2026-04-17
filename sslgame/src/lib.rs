pub mod proto {
    pub mod remote {
        include!(concat!(env!("OUT_DIR"), "/remote.rs"));
    }
}
mod depth_mask_material;
pub mod field;
mod mesh_gen;
mod network_tasks;
mod transform_filter;
mod visualization_tracker;

use crate::field::Field;
use crate::field::robots::{Ball, Robot};
use bevy::prelude::*;
use field::visualizations::AvailableVisualizations;
use bevy_hanabi::{
    Attribute, EffectAsset, ExprWriter, HanabiPlugin, OrientMode, OrientModifier, ScalarType,
    SetAttributeModifier, SpawnerSettings, VectorType,
};

pub fn ssl_game_plugin(app: &mut App) {
    app.add_plugins(field::field_plugin);

    app.insert_resource(RenderSettings {
        field: true,
        robots: RobotRenderSettings::Fallback,
        ball: true,
        visualizations: true,
    });

    app.add_plugins(HanabiPlugin);

    let world = app.world_mut();

    // Particle Effect
    let wind_effect = {
        let writer = ExprWriter::new();

        let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.).expr());

        let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, writer.lit(1.7).expr());

        let init_pos = SetAttributeModifier {
            attribute: Attribute::POSITION,
            value: writer
                .rand(VectorType::VEC3F)
                .mul(writer.lit(Vec3::new(1., 2., 5.))) // z = field width
                .sub(writer.lit(Vec3::new(6., 0., 2.5))) // x = -field_width / 2
                .expr(),
        };

        let min_speed = 5.;
        let max_speed = 7.;
        let init_vel = SetAttributeModifier {
            attribute: Attribute::VELOCITY,
            value: writer
                .lit(Vec3::new(1., 0., 0.))
                .mul(
                    writer.lit(min_speed)
                        + writer.lit(max_speed - min_speed) * writer.rand(ScalarType::Float),
                )
                .expr(),
        };

        let init_scale = SetAttributeModifier {
            attribute: Attribute::SIZE3,
            value: writer.lit(Vec3::new(0.8, 0.05, 1.)).expr(),
        };

        let update_scale = SetAttributeModifier {
            attribute: Attribute::SIZE3,
            value: (writer.lit(Vec3::new(0., 0.05, 1.))
                + (writer.lit(Vec3::new(1., 0., 0.))
                    * writer
                        .attr(Attribute::AGE)
                        .mul(writer.lit(5.))
                        .min(
                            writer
                                .attr(Attribute::LIFETIME)
                                .sub(writer.attr(Attribute::AGE))
                                .mul(writer.lit(5.)),
                        )
                        .min(writer.lit(1.))))
            .expr(),
        };

        let init_color = SetAttributeModifier::new(
            Attribute::COLOR,
            writer.lit(Vec4::new(1., 1., 1., 1.)).pack4x8unorm().expr(),
        );

        let module = writer.finish();

        let wind_effect = EffectAsset::new(1000, SpawnerSettings::rate(20.0.into()), module)
            .with_name("smots_wind")
            .with_alpha_mode(bevy_hanabi::AlphaMode::Opaque)
            .init(init_pos)
            .init(init_vel)
            .init(init_scale)
            .init(init_age)
            .init(init_lifetime)
            .init(init_color)
            .render(OrientModifier::new(OrientMode::AlongVelocity))
            .update(update_scale);

        world.resource_mut::<Assets<EffectAsset>>().add(wind_effect)
    };

    // Materials
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let white_mat_opaque = materials.add(StandardMaterial::from_color(Color::WHITE));
    let white_mat_translucent = materials.add({
        let mut tmp = StandardMaterial::from_color(Color::WHITE);
        tmp.alpha_mode = AlphaMode::Blend;
        tmp
    });

    app.insert_resource(DefaultMaterial {
        opaque: white_mat_opaque,
        translucent: white_mat_translucent,
    });
    app.insert_resource(SmotsWindEffect(wind_effect));

    // Systems
    app.add_systems(
        Update,
        handle_render_settings_change.run_if(resource_changed::<RenderSettings>),
    );
}

#[derive(Clone, Debug, Default)]
pub enum RobotRenderSettings {
    #[default]
    Detailed,
    Fallback,
    Cutout,
    None,
}

#[derive(Resource, Clone, Debug)]
pub struct RenderSettings {
    pub field: bool,
    pub robots: RobotRenderSettings,
    pub ball: bool,
    pub visualizations: bool,
}

impl RenderSettings {
    pub fn full() -> Self {
        RenderSettings {
            field: true,
            robots: RobotRenderSettings::Detailed,
            ball: true,
            visualizations: true,
        }
    }
    pub fn ar() -> Self {
        RenderSettings {
            field: false,
            robots: RobotRenderSettings::Cutout,
            ball: false,
            visualizations: true,
        }
    }
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            field: true,
            robots: RobotRenderSettings::default(),
            ball: true,
            visualizations: true,
        }
    }
}

#[derive(Resource, Debug)]
struct DefaultMaterial {
    pub opaque: Handle<StandardMaterial>,
    pub translucent: Handle<StandardMaterial>,
}

#[derive(Resource, Debug)]
pub struct SmotsWindEffect(pub Handle<EffectAsset>);

#[allow(clippy::type_complexity)]
fn handle_render_settings_change(
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    (q_fields, q_robots, _q_balls): (
        Query<Entity, (With<Field>, With<Mesh3d>)>,
        Query<Entity, With<Robot>>,
        Query<Entity, With<Ball>>,
    ),
) {
    // Remove all potentially outdated entities. They will be recreated automatically.
    // Does not affect visualizations and balls, as they get regenerated periodically anyways.
    if !render_settings.field {
        // The field entity is also used as a marker for data processing, so only the model is removed
        for field_entity in q_fields {
            commands.entity(field_entity).remove::<Mesh3d>();
        }
    }
    q_robots.iter().for_each(|e| commands.entity(e).despawn());
}
