use crate::icons::icon;
use crate::ui::{ImmediateActivate, TextOfComponent, regular_text, tokens};
use bevy::feathers::controls::{ButtonVariant, FeathersButton};
use bevy::feathers::cursor::EntityCursor;
use bevy::feathers::theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

mod host_manager;
mod robots_inspector;
mod vis_inspector;

pub fn sidebar_plugin(app: &mut App) {
    app.add_plugins(host_manager::host_manager_plugin);
    app.add_plugins(robots_inspector::robots_inspector_plugin);
    app.add_plugins(vis_inspector::vis_inspector_plugin);

    app.add_observer(on_field_create);
}

// ======== Inspector ========

/// On a [Sidebar], it stores the last selected inspector. On an inspector button, it marks the inspector this button spawns.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
enum InspectorType {
    Robots,
    #[default]
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

    fn all() -> impl Iterator<Item = Self> {
        use InspectorType::*;
        vec![Robots, Vis].into_iter()
    }
}

/// Component for fields stores how it should be displayed in the sidebar.
#[derive(Component, Clone, Default)]
#[component(immutable)]
pub struct FieldId(pub u8);

// ======== Sidebar ========

/// Marker component for the sidebar column
#[derive(Component, Clone, Default)]
struct Sidebar;

/// Marker component for expanded field entries. There should only ever be one expanded entry in the sidebar, and it should be the first child.
#[derive(Component, Clone, Default)]
struct ExpandedFieldEntry;

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
            ThemeBackgroundColor(tokens::PANEL_BORDER)
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
                InspectorType::default()
                Node {
                    width: px(50),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    padding: px(6),
                }
                ThemeBackgroundColor(tokens::PANEL_BG)
                Children [
                    #PlusButton
                    @FeathersButton {
                        @caption: bsn! { icon(lucide_icons::Icon::Plus, px(24)) },
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
            @caption: bsn! { regular_text(field_id.to_string()) TextFont { font_size: px(20) } },
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

fn expanded_field_entry_scene(
    field_entity: Entity,
    field_id: u8,
    initial_selection: InspectorType,
) -> impl Scene {
    fn inspector_button(inspector_type: InspectorType) -> impl Scene {
        bsn! {
            template_value(inspector_type)
            bevy::ui_widgets::Button
            Node {
                width: percent(100),
                aspect_ratio: {Some(1.0)},
                border_radius: px(4),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
            }
            on(on_inspector_button_click)
            // Hover feedback for unselected buttons
            on(|event: On<Pointer<Enter>>, mut commands: Commands, q_children: Query<&Children>| {
                if let Ok(text_entity) = q_children.get(event.entity).map(|c| c[0]) {
                    commands.entity(text_entity).insert(ThemeTextColor(tokens::TEXT_0));
                }
            })
            on(|event: On<Pointer<Leave>>, mut commands: Commands, (q_parent, q_children): (Query<&ChildOf>, Query<&Children>), q_sidebar: Query<&InspectorType, With<Sidebar>>, q_inspector_button: Query<&InspectorType, Without<Sidebar>>| {
                let button_type = q_inspector_button.get(event.entity).unwrap();
                let selected_type = q_parent.iter_ancestors(event.entity).find_map(|e| q_sidebar.get(e).ok()).unwrap();
                if button_type != selected_type && let Ok(text_entity) = q_children.get(event.entity).map(|c| c[0]) {
                    commands.entity(text_entity).insert(ThemeTextColor(tokens::TEXT_1));
                }
            })
            Children [
                icon(inspector_type.icon(), px(22)) ThemeTextColor(tokens::TEXT_1),
            ]
        }
    }

    let inspector_buttons = InspectorType::all()
        .map(|inspector| {
            if inspector == initial_selection {
                Box::new(bsn! { inspector_button(inspector) ImmediateActivate }) as Box<dyn Scene>
            } else {
                Box::new(inspector_button(inspector))
            }
        })
        .collect::<Vec<_>>();

    bsn! {
        #ExpandedFieldEntry
        ExpandedFieldEntry
        SidebarEntryRepresents(field_entity)
        Node {
            flex_direction: FlexDirection::Column,
            padding: px(3),
            row_gap: px(3),
            align_items: AlignItems::Center,
            border_radius: px(4),
            border: px(1),
        }
        ThemeBorderColor(tokens::SIDEBAR_EXPANDED_FIELD_BORDER)
        ThemeBackgroundColor(tokens::SIDEBAR_EXPANDED_FIELD_BG)
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
            ThemeBackgroundColor(tokens::SIDEBAR_EXPANDED_FIELD_BUTTON)
            on(on_expanded_click)
            Children [ regular_text(field_id.to_string()) TextFont { font_size: px(20) } TextOfComponent(#ExpandedFieldEntry) ],
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                width: percent(100),
            }
            Children [
                {inspector_buttons}
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
    q_last_selected_inspector: Query<&InspectorType>,
) {
    if let Ok(ChildOf(sidebar_entity)) = q_parent.get(click.entity)
        && let Ok(SidebarEntryRepresents(field_entity)) = q_field_entries.get(click.entity)
    {
        let field_id = q_field.get(*field_entity).unwrap().0;

        commands.queue(collapse_expanded_entry(*sidebar_entity));
        // Despawn the old button
        commands.entity(click.entity).despawn();

        // Spawn the expanded entry
        let last_selected_inspector = q_last_selected_inspector.get(*sidebar_entity).unwrap();
        let new_entity = commands
            .spawn_scene(expanded_field_entry_scene(
                *field_entity,
                field_id,
                *last_selected_inspector,
            ))
            .id();
        commands.entity(*sidebar_entity).insert_child(0, new_entity);
    }
}

fn on_expanded_click(click: On<Activate>, mut commands: Commands, q_parent: Query<&ChildOf>) {
    if let Ok(expanded_entry_entity) = q_parent.get(click.entity)
        && let Ok(sidebar_entity) = q_parent.get(expanded_entry_entity.0)
        && let Ok(container_entity) = q_parent.get(sidebar_entity.0)
    {
        commands.queue(collapse_expanded_entry(sidebar_entity.0));
        commands.queue(replace_panel_command(container_entity.0, None::<()>));
    }
}

fn on_inspector_button_click(
    click: On<Activate>,
    mut commands: Commands,
    (q_parent, q_children): (Query<&ChildOf>, Query<&Children>),
    q_inspector_type: Query<&InspectorType>,
    q_entry_field: Query<&SidebarEntryRepresents>,
) {
    let button_entity = click.entity;
    if let Ok(inspector_list_entity) = q_parent.get(button_entity)
        && let Ok(expanded_entry_entity) = q_parent.get(inspector_list_entity.0)
        && let Ok(sidebar_entity) = q_parent.get(expanded_entry_entity.0)
        && let Ok(container_entity) = q_parent.get(sidebar_entity.0)
    {
        let field_entity = q_entry_field.get(expanded_entry_entity.0).unwrap().0;
        let inspector_type = q_inspector_type.get(button_entity).unwrap();
        let inspector_scene = bsn! {
            {inspector_type.scene(field_entity)}
            on(move |_: On<Despawn, Node>, mut commands: Commands, q_children: Query<&Children>| {
                if let Ok(button_children) = q_children.get(button_entity) {
                    commands.entity(button_children[0]).try_insert(ThemeTextColor(tokens::TEXT_1));
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
            .try_insert(ThemeTextColor(tokens::TEXT_0));
        // Set the last selected
        commands.entity(sidebar_entity.0).insert(*inspector_type);
    }
}

fn on_plus_click(
    click: On<Activate>,
    mut commands: Commands,
    q_parent: Query<&ChildOf>,
    mut q_button_variant: Query<&mut ButtonVariant>,
) {
    let button_entity = click.entity;
    if let Ok(ChildOf(sidebar_entity)) = q_parent.get(button_entity)
        && let Ok(ChildOf(container_entity)) = q_parent.get(*sidebar_entity)
        && let Ok(mut button_variant) = q_button_variant.get_mut(button_entity)
    {
        if *button_variant == ButtonVariant::Normal {
            commands.queue(collapse_expanded_entry(*sidebar_entity));
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

fn collapse_expanded_entry(sidebar_entity: Entity) -> impl Command {
    move |world: &mut World| {
        if let Some(sidebar_children) = world.get::<Children>(sidebar_entity)
            && let Some(expanded_entity) = sidebar_children.first()
            && world.get::<ExpandedFieldEntry>(*expanded_entity).is_some()
            && let Some(SidebarEntryRepresents(field_entity)) = world.get(*expanded_entity)
            && let Some(FieldId(field_id)) = world.get(*field_entity)
        {
            let sidebar_entity = sidebar_entity;
            let field_entity = *field_entity;
            let field_id = *field_id;
            world.entity_mut(*expanded_entity).despawn();
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
