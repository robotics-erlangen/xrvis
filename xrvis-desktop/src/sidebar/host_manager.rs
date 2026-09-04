use crate::icons::icon;
use crate::sidebar::FieldId;
use bevy::color::palettes::tailwind::{GREEN_500, RED_500};
use bevy::ecs::template::TemplateContext;
use bevy::feathers::controls::{ButtonVariant, FeathersButton};
use bevy::feathers::theme::{ThemeBackgroundColor, ThemeBorderColor, ThemedText};
use bevy::feathers::tokens;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use derive_more::IntoIterator;
use lucide_icons::Icon;
use sslgame::field::Field;
use sslgame::field::hosts::{
    BallHost, BlueRobotHost, GameStateHost, GeometryFields, GeometryHost, Host, HostConnection,
    YellowRobotHost,
};

pub fn host_manager_plugin(app: &mut App) {
    app.add_observer(on_new_host);
    app.add_observer(on_host_connect);
    app.add_observer(on_field_spawn);
    app.add_observer(on_host_disconnect);
    app.add_observer(on_field_despawn);
}

pub fn scene() -> impl Scene {
    bsn! { HostManager }
}

#[derive(Component, FromTemplate, Clone)]
#[relationship(relationship_target = RepresentedByHostUi)]
struct HostUiRepresentsEntity(Entity);
#[derive(Component, IntoIterator, Clone)]
#[relationship_target(relationship = HostUiRepresentsEntity, linked_spawn)]
struct RepresentedByHostUi(#[into_iterator(owned, ref, ref_mut)] Vec<Entity>);

#[derive(Component)]
struct HostManager;

#[derive(Default)]
struct HostManagerTemplate;
impl FromTemplate for HostManager {
    type Template = HostManagerTemplate;
}
impl Template for HostManagerTemplate {
    type Output = HostManager;

    fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
        let hosts = context.entity.world_scope(|world| {
            let mut q_host = world.query::<(
                &Host,
                Option<&HostConnection>,
                Option<&GeometryFields>,
                Entity,
            )>();
            let mut q_field = world.query::<&FieldId>();
            q_host
                .iter(world)
                .map(|(host, host_conn, geom_fields, host_entity)| {
                    (
                        host.clone(),
                        host_entity,
                        host_conn.is_some(),
                        geom_fields.and_then(|field_ref| {
                            q_field
                                .get(world, field_ref.iter().next().unwrap())
                                .map(|f| f.0)
                                .ok()
                        }),
                    )
                })
                .collect::<Vec<_>>()
        });
        let children: Vec<Box<dyn Scene>> = if hosts.is_empty() {
            vec![Box::new(placeholder_scene())]
        } else {
            hosts
                .into_iter()
                .map(|(host, host_entity, connected, spawned_as)| {
                    Box::new(host_entry_scene(&host, host_entity, connected, spawned_as))
                        as Box<dyn Scene>
                })
                .collect()
        };

        let scene = bsn! {
            Node {
                width: px(300),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                padding: px(6),
                border: {UiRect::right(px(1))},
                overflow: Overflow::scroll_y(),
            }
            ThemeBackgroundColor(tokens::PANE_BODY_BG)
            ThemeBorderColor(tokens::PANE_HEADER_DIVIDER)
            Children [
                {children}
            ]
            on(on_remove_last_entry)
        };
        context.entity.apply_scene(scene)?;
        Ok(HostManager)
    }

    fn clone_template(&self) -> Self {
        HostManagerTemplate
    }
}

// ======== Scenes ========

fn placeholder_scene() -> impl Scene {
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        }
        Children [
            Text("No host found") TextFont {font_size: px(20)},
        ]
    }
}

fn host_entry_scene(
    host: &Host,
    host_entity: Entity,
    connected: bool,
    spawned_as: Option<u8>,
) -> impl Scene {
    bsn! {
        HostUiRepresentsEntity(host_entity)
        @FeathersButton {
            @caption: bsn_list! [
                (
                    Node {
                        height: percent(100),
                        aspect_ratio: {Some(1.0)},
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                    }
                    Children [
                        conn_indicator_scene(connected, spawned_as),
                    ]
                ),
                (
                    Text({host.to_string()})
                    ThemedText
                    // Padding to fix visual alignment
                    Node {padding: UiRect::top(px(1.5))}
                ),
                Node {flex_grow: 1.0},
                (
                    @FeathersButton {
                        @caption: bsn! {
                            icon({if spawned_as.is_some() {Icon::Link2Off} else {Icon::Link2}}, px(12))
                        },
                    }
                    Node {
                        height: percent(100),
                    }
                    Visibility::Hidden
                    on(on_connect_click)
                ),
                (
                    @FeathersButton {
                        @caption: bsn! {
                            icon((if spawned_as.is_some() {Icon::CornerUpLeft} else {Icon::CornerRightDown}), px(12))
                        },
                    }
                    Node {
                        height: percent(100),
                    }
                    Visibility::Hidden
                    on(on_spawn_click)
                ),
            ],
            @variant: ButtonVariant::Plain,
        }
        Node {
            width: percent(100),
            justify_content: JustifyContent::Start,
            column_gap: px(4),
            padding: px(3),
        }
        on(on_host_hover)
        on(on_host_unhover)
    }
}

fn conn_indicator_scene(connected: bool, spawned_as: Option<u8>) -> impl Scene {
    bsn! {
        conn_indicator_connected_patch(connected)
        conn_indicator_spawned_patch(spawned_as)
    }
}

fn conn_indicator_connected_patch(connected: bool) -> impl Scene {
    let color = if connected { GREEN_500 } else { RED_500 };
    bsn! {
        BackgroundColor(color)
    }
}

fn conn_indicator_spawned_patch(spawned_as: Option<u8>) -> impl Scene {
    if let Some(field_id) = spawned_as {
        Box::new(bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                border_radius: px(3),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
            }
            Children [
                Text({field_id.to_string()}) TextFont {font_size: px(12)},
            ]
        }) as Box<dyn Scene>
    } else {
        Box::new(bsn! {
            Node {
                width: px(6),
                height: px(6),
                border_radius: px(3),
            }
        })
    }
}

// ======== Add/Remove hosts ========

fn on_new_host(
    host_add: On<Add, Host>,
    mut commands: Commands,
    q_manager: Query<(&Children, Entity), With<HostManager>>,
    q_host: Query<&Host>,
    q_placeholder: Query<Entity, Without<HostUiRepresentsEntity>>,
) {
    let new_host = q_host.get(host_add.entity).unwrap();
    for (manager_children, manager_entity) in q_manager.iter() {
        let scene = host_entry_scene(new_host, host_add.entity, false, None);
        commands.spawn_scene(scene).insert(ChildOf(manager_entity));
        // Despawn the placeholder afterward so it doesn't get recreated by on_remove_last_entry
        if let Ok(placeholder_entity) = q_placeholder.get(manager_children[0]) {
            commands.entity(placeholder_entity).despawn();
        }
    }
}

fn on_remove_last_entry(remove: On<Remove, Children>, mut commands: Commands) {
    // Atomic add + child, to avoid leaving a ghost scene when the event is coming from a full despawn
    commands.entity(remove.entity).queue_silenced(
        move |entity_world: EntityWorldMut| -> Result<(), BevyError> {
            let entity = entity_world.id();
            entity_world
                .into_world_mut()
                .spawn_scene(placeholder_scene())?
                .insert(ChildOf(entity));
            Ok(())
        },
    );
}

// ======== Button interactions ========

fn on_host_hover(
    host_hover: On<Pointer<Enter>>,
    mut commands: Commands,
    q_children: Query<&Children>,
) {
    if let Ok(children) = q_children.get(host_hover.entity)
        && let Some([connect_button, spawn_button]) = children.last_chunk()
    {
        commands
            .entity(*connect_button)
            .insert(Visibility::Inherited);
        commands.entity(*spawn_button).insert(Visibility::Inherited);
    }
}

fn on_host_unhover(
    host_unhover: On<Pointer<Leave>>,
    mut commands: Commands,
    q_children: Query<&Children>,
) {
    if let Ok(children) = q_children.get(host_unhover.entity)
        && let Some([connect_button, spawn_button]) = children.last_chunk()
    {
        commands.entity(*connect_button).insert(Visibility::Hidden);
        commands.entity(*spawn_button).insert(Visibility::Hidden);
    }
}

#[allow(clippy::type_complexity)]
fn on_connect_click(
    click: On<Activate>,
    mut commands: Commands,
    q_parent: Query<&ChildOf>,
    q_entry: Query<&HostUiRepresentsEntity>,
    q_host: Query<(&Host, Option<&HostConnection>, Entity)>,
) {
    if let Ok(entry_entity) = q_parent.get(click.entity)
        && let Ok(host_ref) = q_entry.get(entry_entity.0)
        && let Ok((host, host_conn, host_entity)) = q_host.get(host_ref.0)
    {
        if host_conn.is_some() {
            commands.entity(host_entity).remove::<HostConnection>();
        } else {
            commands.entity(host_entity).insert(host.start_connection());
        }
    }
}

#[allow(clippy::type_complexity)]
fn on_spawn_click(
    click: On<Activate>,
    mut commands: Commands,
    q_parent: Query<&ChildOf>,
    q_entry: Query<&HostUiRepresentsEntity>,
    q_host: Query<(
        &Host,
        Option<&HostConnection>,
        Option<&GeometryFields>,
        Entity,
    )>,
    q_field: Query<&FieldId>,
) {
    if let Ok(entry_entity) = q_parent.get(click.entity)
        && let Ok(host_ref) = q_entry.get(entry_entity.0)
        && let Ok((host, host_conn, host_usages, host_entity)) = q_host.get(host_ref.0)
    {
        if let Some(fields) = host_usages {
            // This UI will only ever spawn one, but the backend supports multiple
            for field_entity in fields.iter() {
                commands.entity(field_entity).despawn();
            }
        } else {
            // Autoconnect for one-click placement
            if host_conn.is_none() {
                commands.entity(host_entity).insert(host.start_connection());
            }

            let mut current_field_ids = q_field.iter().collect::<Vec<_>>();
            current_field_ids.sort_by_key(|f| f.0);
            let free_id = current_field_ids
                .iter()
                .enumerate()
                .find_map(|(i, id)| (i != id.0 as usize).then_some(i))
                .unwrap_or(current_field_ids.len());

            commands.spawn((
                Field,
                FieldId(free_id as u8),
                GeometryHost(host_entity),
                GameStateHost(host_entity),
                BallHost(host_entity),
                YellowRobotHost(host_entity),
                BlueRobotHost(host_entity),
                Transform::from_xyz(0.0, 0.0, 0.0),
            ));
        }

        // Relayout all the fields. Defer into a command so that it acts on the new state.
        commands.queue(|world: &mut World| {
            let mut q_field = world.query::<(&FieldId, &mut Transform)>();

            let mut fields = q_field
                .iter_mut(world)
                .map(|(field_id, transform)| (field_id.0, transform))
                .collect::<Vec<_>>();
            fields.sort_unstable_by_key(|(id, _)| *id);

            let fields_len = fields.len();
            for (i, (_, transform)) in fields.iter_mut().enumerate() {
                transform.translation.z = (i * 10) as f32 - ((fields_len - 1) as f32 * 5.0);
            }
        });
    }
}

// ======== Connection indicator ========

fn on_host_connect(
    host_connect: On<Add, HostConnection>,
    mut commands: Commands,
    q_host: Query<&RepresentedByHostUi>,
    q_children: Query<&Children>,
    mut q_text: Query<&mut Text>,
) {
    let Ok(ui_entry_entities) = q_host.get(host_connect.entity) else {
        return;
    };

    for entry_entity in ui_entry_entities {
        if let Ok(entry_children) = q_children.get(*entry_entity) {
            // Update the connection indicator
            if let Ok(conn_indicator_inner) = q_children.get(entry_children[0]) {
                commands
                    .entity(conn_indicator_inner[0])
                    .apply_scene(conn_indicator_connected_patch(true));
            }

            // Update the spawn button
            if let Ok(spawn_caption_entity) = q_children.get(entry_children[3])
                && let Ok(mut spawn_caption) = q_text.get_mut(spawn_caption_entity[0])
            {
                spawn_caption.0 = Icon::Link2Off.unicode().into();
            }
        }
    }
}

fn on_field_spawn(
    field_spawn: On<Add, FieldId>,
    mut commands: Commands,
    q_field: Query<(&FieldId, &GeometryHost)>,
    q_host: Query<&RepresentedByHostUi>,
    q_children: Query<&Children>,
    mut q_text: Query<&mut Text>,
) {
    if let Ok((field_id, geometry_host)) = q_field.get(field_spawn.entity)
        && let Ok(ui_entry_entities) = q_host.get(geometry_host.0)
    {
        for entry_entity in ui_entry_entities {
            if let Ok(entry_children) = q_children.get(*entry_entity) {
                // Update the connection indicator
                if let Ok(indicator) = q_children.get(entry_children[0]) {
                    commands
                        .entity(indicator[0])
                        .apply_scene(conn_indicator_spawned_patch(Some(field_id.0)));
                }

                // Update the spawn button
                if let Ok(spawn_caption_entity) = q_children.get(entry_children[4])
                    && let Ok(mut spawn_caption) = q_text.get_mut(spawn_caption_entity[0])
                {
                    spawn_caption.0 = Icon::CornerUpLeft.unicode().into();
                }
            }
        }
    }
}

fn on_host_disconnect(
    host_disconnect: On<Remove, HostConnection>,
    mut commands: Commands,
    q_host: Query<&RepresentedByHostUi>,
    q_children: Query<&Children>,
    mut q_text: Query<&mut Text>,
) {
    let Ok(ui_entry_entities) = q_host.get(host_disconnect.entity) else {
        return;
    };

    for entry_entity in ui_entry_entities {
        if let Ok(entry_children) = q_children.get(*entry_entity) {
            // Update the connection indicator
            if let Ok(conn_indicator_inner) = q_children.get(entry_children[0]) {
                // queue_silenced to handle full host despawns
                commands.entity(conn_indicator_inner[0]).queue_silenced(
                    move |mut entity: EntityWorldMut| {
                        entity.apply_scene(conn_indicator_connected_patch(false))
                    },
                );
            }

            // Update the spawn button
            if let Ok(spawn_caption_entity) = q_children.get(entry_children[3])
                && let Ok(mut spawn_caption) = q_text.get_mut(spawn_caption_entity[0])
            {
                spawn_caption.0 = Icon::Link2.unicode().into();
            }
        }
    }
}

fn on_field_despawn(
    field_despawn: On<Remove, FieldId>,
    mut commands: Commands,
    q_field: Query<&GeometryHost>,
    q_host: Query<&RepresentedByHostUi>,
    q_children: Query<&Children>,
    mut q_text: Query<&mut Text>,
) {
    if let Ok(geometry_host) = q_field.get(field_despawn.entity)
        && let Ok(ui_entry_entities) = q_host.get(geometry_host.0)
    {
        for entry_entity in ui_entry_entities {
            if let Ok(entry_children) = q_children.get(*entry_entity) {
                // Update the connection indicator
                if let Ok(indicator) = q_children.get(entry_children[0]) {
                    // queue_silenced to handle full host despawns
                    commands.entity(indicator[0]).queue_silenced(
                        move |mut entity: EntityWorldMut| {
                            entity
                                .despawn_children()
                                .apply_scene(conn_indicator_spawned_patch(None))
                        },
                    );
                }

                // Update the spawn button
                if let Ok(spawn_caption_entity) = q_children.get(entry_children[4])
                    && let Ok(mut spawn_caption) = q_text.get_mut(spawn_caption_entity[0])
                {
                    spawn_caption.0 = Icon::CornerRightDown.unicode().into();
                }
            }
        }
    }
}
