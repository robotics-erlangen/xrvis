use bevy::feathers::controls::{ButtonVariant, FeathersButton};
use bevy::feathers::theme::ThemeBackgroundColor;
use bevy::feathers::tokens;
use bevy::prelude::*;

pub fn sidebar_plugin(app: &mut App) {
    app.add_observer(on_field_create);
    app.add_observer(on_entry_selected);
    app.add_observer(on_entry_deselected);
}

#[derive(Component, Clone, Default)]
struct Sidebar;

#[derive(Component, Clone, Default)]
#[component(immutable)]
pub struct FieldId(pub u8);

#[derive(Component, FromTemplate, Clone)]
#[relationship(relationship_target = SidebarEntrySelectedBy)]
struct SelectedSidebarEntry(Entity);
#[derive(Component, Clone)]
#[relationship_target(relationship = SelectedSidebarEntry)]
struct SidebarEntrySelectedBy(Entity);

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
                        padding: px(4),
                        aspect_ratio: {Some(1.0)},
                    }
                    on(on_entry_click)
                ]
            ),
            (separator()),
            // Open panel will be spawned here
        ]
    }
}

fn on_field_create(
    field_add: On<Add, FieldId>,
    mut commands: Commands,
    sidebar: Single<(Entity, &Children), With<Sidebar>>,
    q_field: Query<&FieldId>,
) {
    let (sidebar_entity, sidebar_children) = *sidebar;
    // Detach the old plus button. The action is deferred, so the children list stays unchanged.
    commands
        .entity(sidebar_entity)
        .detach_child(*sidebar_children.last().unwrap());

    let field_id = q_field.get(field_add.entity).unwrap().0;

    // Spawn new field button
    let new_entry_entity = commands
        .spawn_scene(bsn! {
            @FeathersButton {
                @caption: bsn! { Text({field_id.to_string()}) TextFont { font_size: px(20) } },
                @variant: ButtonVariant::Normal,
            }
            on(on_entry_click)
        })
        .insert(SidebarEntryRepresents(field_add.entity))
        .id();
    commands.entity(sidebar_entity).add_child(new_entry_entity);

    // Re-attach the plus button at the end
    commands
        .entity(sidebar_entity)
        .add_child(*sidebar_children.last().unwrap());
}

fn on_entry_click(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    q_entry: Query<&SidebarEntrySelectedBy>,
    q_parent: Query<&ChildOf>,
) {
    if let Ok(SidebarEntrySelectedBy(sidebar_entity)) = q_entry.get(click.entity) {
        // Already selected -> deselect
        commands
            .entity(*sidebar_entity)
            .remove::<SelectedSidebarEntry>();
    } else if let Ok(ChildOf(sidebar_entity)) = q_parent.get(click.entity) {
        // Not yet selected -> select
        commands
            .entity(*sidebar_entity)
            .insert(SelectedSidebarEntry(click.entity));
    }
}

fn on_entry_selected(
    entry_selected: On<Add, SidebarEntrySelectedBy>,
    mut commands: Commands,
    q_parent: Query<&ChildOf>,
    q_field_entries: Query<&SidebarEntryRepresents>,
) {
    // Highlight button
    commands
        .entity(entry_selected.entity)
        .insert(ButtonVariant::Primary);

    // Spawn panel
    if let Ok(ChildOf(sidebar_entity)) = q_parent.get(entry_selected.entity)
        && let Ok(ChildOf(container_entity)) = q_parent.get(*sidebar_entity)
    {
        if let Ok(SidebarEntryRepresents(field_entity)) = q_field_entries.get(entry_selected.entity)
        {
            let inspector_entity = commands
                .spawn_scene(crate::field_inspector::scene(*field_entity))
                .id();
            commands
                .entity(*container_entity)
                .add_child(inspector_entity);
        } else {
            let manager_entity = commands.spawn_scene(crate::host_manager::scene()).id();
            commands.entity(*container_entity).add_child(manager_entity);
        }
    }
}

fn on_entry_deselected(
    entry_deselected: On<Remove, SidebarEntrySelectedBy>,
    mut commands: Commands,
    q_children: Query<&Children>,
    q_parent: Query<&ChildOf>,
) {
    // Remove button highlight
    commands
        .entity(entry_deselected.entity)
        .try_insert(ButtonVariant::Normal);

    // Despawn open panel
    // - Container
    //   - Sidebar
    //     - Entry <- start
    //   - Separator
    //   - Panel <- target
    if let Ok(ChildOf(sidebar_entity)) = q_parent.get(entry_deselected.entity)
        && let Ok(ChildOf(container_entity)) = q_parent.get(*sidebar_entity)
        && let Ok(container_children) = q_children.get(*container_entity)
        && container_children.len() == 3
        && let Some(panel_entity) = container_children.last()
    {
        commands.entity(*panel_entity).despawn();
    }
}
