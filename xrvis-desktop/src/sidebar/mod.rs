use bevy::ecs::system::{IntoObserverSystem, ObserverSystem};
use bevy::feathers::controls::{ButtonVariant, FeathersButton};
use bevy::feathers::cursor::EntityCursor;
use bevy::feathers::theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor};
use bevy::feathers::tokens;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

mod host_manager;
mod robots_inspector;
mod vis_inspector;

#[derive(Clone, Copy, Debug)]
enum InspectorType {
    Robots,
    Vis,
}

impl InspectorType {
    fn icon(&self) -> lucide_icons::Icon {
        match self {
            InspectorType::Robots => lucide_icons::Icon::Bot,
            InspectorType::Vis => lucide_icons::Icon::Tangent,
        }
    }

    fn scene(&self, field_entity: Entity) -> Box<dyn Scene> {
        match self {
            InspectorType::Robots => Box::new(robots_inspector::scene(field_entity)),
            InspectorType::Vis => Box::new(vis_inspector::scene(field_entity)),
        }
    }
}

// ======== Util ========

/// Reference to the [Text] component for this UI element. Useful to avoid traversing the internal hierarchy of premade components like [FeathersCheckbox].
#[derive(Component, Clone, Copy)]
#[relationship_target(relationship = TextOfComponent)]
struct ComponentText(Entity);
#[derive(Component, FromTemplate, Clone, Copy)]
#[relationship(relationship_target = ComponentText)]
struct TextOfComponent(pub Entity);

/// Triggers [Activate] when added, useful for simulate an immediate click when spawning a button.
#[derive(Component, Clone, Copy, Default)]
struct ImmediateActivate;

fn handle_immediate_activate(
    mut commands: Commands,
    q_buttons: Query<Entity, (With<ImmediateActivate>, Added<ImmediateActivate>)>, // With<> is here because Added<> is slow on its own
) {
    for entity in q_buttons {
        commands.entity(entity).remove::<ImmediateActivate>();
        commands.trigger(Activate { entity });
    }
}

// ======== Sidebar ========

pub fn sidebar_plugin(app: &mut App) {
    app.add_plugins(host_manager::host_manager_plugin);
    app.add_plugins(robots_inspector::robots_inspector_plugin);
    app.add_plugins(vis_inspector::vis_inspector_plugin);

    app.add_observer(on_field_create);
    app.add_systems(PostUpdate, handle_immediate_activate);
}

#[derive(Component, Clone, Default)]
struct Sidebar;

#[derive(Component, Clone, Default)]
#[component(immutable)]
pub struct FieldId(pub u8);

#[derive(Component, FromTemplate, Clone)]
#[relationship(relationship_target = RepresentedBySidebarEntry)]
struct SidebarEntryRepresents(Entity);
#[derive(Component, Clone)]
#[relationship_target(relationship = SidebarEntryRepresents, linked_spawn)]
struct RepresentedBySidebarEntry(Vec<Entity>);

pub fn scene() -> impl Scene {
    fn separator() -> impl Scene {
        bsn! {
            Node {
                width: px(1),
                height: percent(100),
            }
            ThemeBackgroundColor(tokens::PANE_HEADER_DIVIDER)
        }
    }

    bsn! {
        #SidebarContainer
        Node {
            height: percent(100),
        }
        Children [
            (
                #Sidebar
                Sidebar
                Node {
                    width: px(50),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    padding: px(6),
                }
                ThemeBackgroundColor(tokens::PANE_BODY_BG)
                Children [
                    #PlusButton
                    @FeathersButton {
                        @caption: bsn! { Text("+") TextFont { font_size: px(30) } },
                        @variant: ButtonVariant::Normal,
                    }
                    Node {
                        width: percent(100),
                        height: Val::Auto,
                        aspect_ratio: {Some(1.0)},
                    }
                    on(on_plus_click)
                ]
            ),
            (separator()),
            // Open panel will be spawned here
        ]
    }
}

fn collapsed_field_entry_scene(field_entity: Entity, field_id: u8) -> impl Scene {
    bsn! {
        #CollapsedFieldEntry
        SidebarEntryRepresents(field_entity)
        @FeathersButton {
            @caption: bsn! { Text({field_id.to_string()}) TextFont { font_size: px(20) } },
            @variant: ButtonVariant::Normal,
        }
        Node {
            width: percent(100),
            height: Val::Auto,
            aspect_ratio: {Some(1.0)},
        }
        on(on_collapsed_click)
    }
}

fn expanded_field_entry_scene(field_entity: Entity, field_id: u8) -> impl Scene {
    // TODO: Hover feedback
    fn inspector_button(inspector: InspectorType, field_entity: Entity) -> impl Scene {
        bsn! {
            bevy::ui_widgets::Button
            Node {
                width: percent(100),
                aspect_ratio: {Some(1.0)},
                border_radius: px(4),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
            }
            on(inspector_click_observer(inspector, field_entity))
            Children [
                crate::icons::icon(inspector.icon(), px(22)) ThemeTextColor(tokens::TEXT_MAIN),
            ]
        }
    }

    bsn! {
        #ExpandedFieldEntry
        SidebarEntryRepresents(field_entity)
        Node {
            flex_direction: FlexDirection::Column,
            padding: px(3),
            row_gap: px(3),
            align_items: AlignItems::Center,
            border_radius: px(4),
            border: px(1),
        }
        ThemeBorderColor(tokens::BUTTON_PRIMARY_BG)
        Children [
            bevy::ui_widgets::Button
            EntityCursor::System(bevy::window::SystemCursorIcon::Pointer)
            Hovered::default()
            Node {
                width: percent(100),
                aspect_ratio: {Some(1.0)},
                border_radius: px(4),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
            }
            ThemeBackgroundColor(tokens::BUTTON_PRIMARY_BG)
            on(on_expanded_click)
            Children [ Text({field_id.to_string()}) TextFont { font_size: px(20) } TextOfComponent(#ExpandedFieldEntry) ],
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                width: percent(100),
            }
            Children [
                #RobotInspectorButton inspector_button(InspectorType::Robots, field_entity),
                #VisInspectorButton inspector_button(InspectorType::Vis, field_entity) ImmediateActivate,
            ]
        ]
    }
}

fn on_field_create(
    field_add: On<Add, FieldId>,
    mut commands: Commands,
    sidebar: Single<(Entity, &Children), With<Sidebar>>,
    q_field: Query<(&FieldId, Entity)>,
) {
    let (sidebar_entity, sidebar_children) = *sidebar;
    let (field_id, field_entity) = q_field.get(field_add.entity).unwrap();

    // Spawn new field button
    let new_entry_entity = commands
        .spawn_scene(collapsed_field_entry_scene(field_entity, field_id.0))
        .id();
    commands
        .entity(sidebar_entity)
        .insert_child(sidebar_children.len() - 1, new_entry_entity);
}

fn on_collapsed_click(
    click: On<Activate>,
    mut commands: Commands,
    q_parent: Query<&ChildOf>,
    q_field_entries: Query<&SidebarEntryRepresents>,
    q_field: Query<&FieldId>,
) {
    if let Ok(ChildOf(sidebar_entity)) = q_parent.get(click.entity)
        && let Ok(SidebarEntryRepresents(field_entity)) = q_field_entries.get(click.entity)
    {
        let field_id = q_field.get(*field_entity).unwrap().0;

        // Despawn the old button
        commands.entity(click.entity).despawn();

        // Spawn the expanded entry
        // TODO: Keep the inspector selection of the last expanded entry
        let new_entity = commands
            .spawn_scene(expanded_field_entry_scene(*field_entity, field_id))
            .id();
        commands.entity(*sidebar_entity).insert_child(0, new_entity);
    }
}

fn on_expanded_click(click: On<Activate>, mut commands: Commands, q_parent: Query<&ChildOf>) {
    if let Ok(expanded_entry_entity) = q_parent.get(click.entity)
        && let Ok(sidebar_entity) = q_parent.get(expanded_entry_entity.0)
        && let Ok(container_entity) = q_parent.get(sidebar_entity.0)
    {
        commands.queue(collapse_expanded_entry(expanded_entry_entity.0));
        commands.queue(replace_panel_command(container_entity.0, None::<()>));
    }
}

fn inspector_click_observer(
    inspector: InspectorType,
    field_entity: Entity,
) -> impl ObserverSystem<Activate, ()> + Clone {
    let on_inspector_click =
        move |click: On<Activate>,
              mut commands: Commands,
              (q_parent, q_children): (Query<&ChildOf>, Query<&Children>)| {
            let button_entity = click.entity;
            if let Ok(inspector_list_entity) = q_parent.get(button_entity)
                && let Ok(expanded_entry_entity) = q_parent.get(inspector_list_entity.0)
                && let Ok(sidebar_entity) = q_parent.get(expanded_entry_entity.0)
                && let Ok(container_entity) = q_parent.get(sidebar_entity.0)
            {
                let inspector_scene = bsn! {
                    {inspector.scene(field_entity)}
                    on(move |_: On<Despawn, Node>, mut commands: Commands, q_children: Query<&Children>| {
                        if let Ok(button_children) = q_children.get(button_entity) {
                            commands.entity(button_children[0]).try_insert(ThemeTextColor(tokens::TEXT_MAIN));
                        }
                    })
                };
                commands.queue(replace_panel_command(
                    container_entity.0,
                    Some(inspector_scene),
                ));
                let text_entity = q_children.get(button_entity).unwrap()[0];
                commands
                    .entity(text_entity)
                    .try_insert(ThemeTextColor(tokens::BUTTON_TEXT));
            }
        };
    IntoObserverSystem::into_system(on_inspector_click)
}

fn on_plus_click(
    click: On<Activate>,
    mut commands: Commands,
    (q_parent, q_children): (Query<&ChildOf>, Query<&Children>),
    mut q_button_variant: Query<&mut ButtonVariant>,
) {
    let button_entity = click.entity;
    if let Ok(ChildOf(sidebar_entity)) = q_parent.get(button_entity)
        && let Ok(ChildOf(container_entity)) = q_parent.get(*sidebar_entity)
        && let Ok(mut button_variant) = q_button_variant.get_mut(button_entity)
    {
        if *button_variant == ButtonVariant::Normal {
            // TODO: This should happen automatically
            let maybe_expanded_entry = q_children.get(*sidebar_entity).unwrap()[0];
            commands.queue(collapse_expanded_entry(maybe_expanded_entry));
            commands.queue(replace_panel_command(
                *container_entity,
                Some(bsn! {
                    host_manager::scene()
                    on(move |_: On<Despawn, Node>, mut commands: Commands| {
                        commands.entity(button_entity).try_insert(ButtonVariant::Normal);
                    })
                }),
            ));
            commands
                .entity(button_entity)
                .try_insert(ButtonVariant::Primary);
            *button_variant = ButtonVariant::Primary;
        } else if *button_variant == ButtonVariant::Primary {
            commands.queue(replace_panel_command(*container_entity, None::<()>));
        }
    }
}

fn collapse_expanded_entry(expanded_entity: Entity) -> impl Command {
    move |world: &mut World| {
        if let Some(ChildOf(sidebar_entity)) = world.get(expanded_entity)
            && let Some(SidebarEntryRepresents(field_entity)) = world.get(expanded_entity)
            && let Some(FieldId(field_id)) = world.get(*field_entity)
        {
            let sidebar_entity = *sidebar_entity;
            let field_entity = *field_entity;
            let field_id = *field_id;
            world.entity_mut(expanded_entity).despawn();
            let collapsed_entity = world
                .spawn_scene(collapsed_field_entry_scene(field_entity, field_id))
                .unwrap()
                .id();
            world
                .entity_mut(sidebar_entity)
                .insert_child(0, collapsed_entity);
        }
    }
}

fn replace_panel_command(container_entity: Entity, new_panel: Option<impl Scene>) -> impl Command {
    move |world: &mut World| {
        let container_children = world.get_mut::<Children>(container_entity).unwrap();
        if let Some(&old_panel_entity) = container_children.get(2) {
            world.entity_mut(old_panel_entity).despawn();
        }
        if let Some(new_panel) = new_panel {
            world
                .spawn_scene(new_panel)
                .unwrap()
                .insert(ChildOf(container_entity));
        }
    }
}
