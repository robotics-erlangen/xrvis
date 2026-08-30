use bevy::app::PropagateOver;
use bevy::ecs::template::{EntityTemplate, TemplateContext};
use bevy::feathers::controls::{
    ButtonVariant, FeathersButton, FeathersCheckbox, FeathersDisclosureToggle, FeathersListView,
};
use bevy::feathers::theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor, ThemedText};
use bevy::feathers::tokens;
use bevy::feathers::tokens::CHECKBOX_TEXT_DISABLED;
use bevy::prelude::*;
use bevy::scene::SceneFunction;
use bevy::ui::Checked;
use bevy::ui_widgets::ValueChange;
use derive_more::IntoIterator;
use sslgame::field::hosts::{BlueRobotHost, HostConnection, YellowRobotHost};
use sslgame::field::visualizations::{
    InactiveVisualization, VisualizationId, VisualizationInstance, VisualizationName,
    VisualizationSourceId, VisualizationSourceName, VisualizationUsages,
};

pub fn field_inspector_plugin(app: &mut App) {
    app.add_observer(on_new_vis_source);
    app.add_observer(on_new_vis);
    app.add_observer(on_source_name_insert);
    app.add_observer(on_vis_name_insert);
    app.add_observer(on_vis_inactive);
    app.add_observer(on_vis_active);
}

pub fn scene(field_entity: Entity) -> impl Scene {
    bsn! {
        FieldInspector { field_entity }
    }
}

/// References the field that this inspector interacts with. Should always be used in [bsn!], so the [FieldInspectorTemplate] can initialize the UI.
#[derive(Component, Clone, Copy)]
#[relationship(relationship_target = FieldInspectedBy)]
struct FieldInspector(Entity);
#[derive(Component, Clone, Copy)]
#[relationship_target(relationship = FieldInspector, linked_spawn)]
struct FieldInspectedBy(Entity);

/// Custom template for [FieldInspector] that creates the initial UI using the current state of the referenced field.
#[derive(Default)]
struct FieldInspectorTemplate {
    field_entity: EntityTemplate,
}
impl FromTemplate for FieldInspector {
    type Template = FieldInspectorTemplate;
}
impl Template for FieldInspectorTemplate {
    type Output = FieldInspector;

    fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
        let field_entity = self.field_entity.build_template(context)?;

        let scene = context.entity.world_scope(|world| {
            let field_entity = world.get_entity(field_entity).unwrap();
            let yellow_host_ref = field_entity.get::<YellowRobotHost>();
            let blue_host_ref = field_entity.get::<BlueRobotHost>();
            let field_entity = field_entity.id();

            let (host_tabs, selected_host) = host_tabs(yellow_host_ref, blue_host_ref);
            let vis_list = world
                .run_system_cached_with(vis_list_for_host, (selected_host.unwrap(), field_entity))
                .unwrap();

            bsn! {
                Node {
                    width: px(300),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    padding: px(6),
                    border: {UiRect::right(px(1))},
                    overflow: Overflow::scroll_y(),
                }
                VisUiRepresentsEntity(field_entity)
                ThemeBackgroundColor(tokens::PANE_BODY_BG)
                ThemeBorderColor(tokens::PANE_HEADER_DIVIDER)
                Children [
                    (
                        Node {
                            width: percent(100),
                            column_gap: px(6),
                        }
                        Children [{host_tabs}]
                    ),
                    (vis_list),
                ]
            }
        });
        context.entity.apply_scene(scene)?;
        Ok(FieldInspector(field_entity))
    }

    fn clone_template(&self) -> Self {
        Self {
            field_entity: self.field_entity,
        }
    }
}

// ======== Util relationships ========

/// Reference to the [Text] component for this UI element. Useful to avoid traversing the internal hierarchy of premade components like [FeathersCheckbox].
#[derive(Component, Clone, Copy)]
#[relationship_target(relationship = TextOfComponent)]
struct ComponentText(Entity);
#[derive(Component, FromTemplate, Clone, Copy)]
#[relationship(relationship_target = ComponentText)]
struct TextOfComponent(pub Entity);

/// Used to link UI elements to their in-world counterparts
/// - Inspector -> Field (Duplicate of [FieldInspector])
/// - Tab -> Host root
/// - SourceUI -> Source
/// - VisUI -> Visualization
#[derive(Component, FromTemplate, Clone)]
#[relationship(relationship_target = RepresentedByVisUi)]
struct VisUiRepresentsEntity(Entity);
#[derive(Component, IntoIterator, Clone)]
#[relationship_target(relationship = VisUiRepresentsEntity, linked_spawn)]
struct RepresentedByVisUi(#[into_iterator(owned, ref, ref_mut)] Vec<Entity>);

// ======== Initial state ========

/// Returns: (Tab buttons, selected host)
fn host_tabs(
    yellow_host_ref: Option<&YellowRobotHost>,
    blue_host_ref: Option<&BlueRobotHost>,
) -> (Box<dyn SceneList>, Option<Entity>) {
    // TODO: Button colors?
    match (yellow_host_ref.map(|h| h.0), blue_host_ref.map(|h| h.0)) {
        (None, None) => (
            Box::new(bsn_list! [ Text({"Field with no robot hosts!".to_owned()}) ThemedText ]),
            None,
        ),
        (Some(yellow_host_entity), None) => (
            Box::new(bsn_list! [
                VisUiRepresentsEntity(yellow_host_entity)
                on(on_host_tab_click)
                @FeathersButton {
                    @caption: bsn! { Text({"Yellow".to_owned()}) ThemedText },
                    @variant: ButtonVariant::Normal,
                }
            ]),
            Some(yellow_host_entity),
        ),
        (None, Some(blue_host_entity)) => (
            Box::new(bsn_list! [
                VisUiRepresentsEntity(blue_host_entity)
                on(on_host_tab_click)
                @FeathersButton {
                    @caption: bsn! { Text({"Blue".to_owned()}) ThemedText },
                    @variant: ButtonVariant::Normal,
                }
            ]),
            Some(blue_host_entity),
        ),
        (Some(yellow_host_entity), Some(blue_host_entity))
            if yellow_host_entity == blue_host_entity =>
        {
            (
                Box::new(bsn_list! [
                    VisUiRepresentsEntity(yellow_host_entity)
                    on(on_host_tab_click)
                    @FeathersButton {
                        @caption: bsn! { Text({"Yellow + Blue".to_owned()}) ThemedText },
                        @variant: ButtonVariant::Normal,
                    }
                ]),
                Some(yellow_host_entity),
            )
        }
        (Some(yellow_host_entity), Some(blue_host_entity)) => (
            Box::new(bsn_list! [
                VisUiRepresentsEntity(yellow_host_entity)
                on(on_host_tab_click)
                @FeathersButton {
                    @caption: bsn! { Text({"Yellow".to_owned()}) ThemedText },
                    @variant: ButtonVariant::Normal,
                },
                VisUiRepresentsEntity(blue_host_entity)
                on(on_host_tab_click)
                @FeathersButton {
                    @caption: bsn! { Text({"Blue".to_owned()}) ThemedText },
                    @variant: ButtonVariant::Plain,
                }
            ]),
            Some(yellow_host_entity),
        ),
    }
}

fn vis_list_for_host(
    In((host_entity, field_entity)): In<(Entity, Entity)>,
    q_source: Query<(
        &VisualizationSourceId,
        Option<&VisualizationSourceName>,
        Entity,
        Option<&Children>,
    )>,
    q_vis: Query<(
        &VisualizationId,
        Option<&VisualizationName>,
        Option<&VisualizationUsages>,
        Entity,
    )>,
    q_children: Query<&Children>,
    q_parent: Query<&ChildOf>,
) -> impl Scene + use<> {
    let host_children = q_children.get(host_entity).into_iter().flatten();
    let source_ui_scenes = q_source
        .iter_many(host_children)
        .sort_unstable_by::<Option<&VisualizationSourceName>>(|name_a, name_b| {
            name_a
                .map(|a| a.0.to_lowercase())
                .cmp(&name_b.map(|b| b.0.to_lowercase()))
        })
        .map(|(source_id, source_name, source_entity, source_children)| {
            // Build the visualization list for this source
            let vis_ui_scenes = q_vis
                .iter_many(source_children.into_iter().flatten())
                .sort_unstable_by::<Option<&VisualizationName>>(|name_a, name_b| {
                    name_a
                        .map(|a| a.0.to_lowercase())
                        .cmp(&name_b.map(|b| b.0.to_lowercase()))
                })
                .map(|(vis_id, vis_name, vis_usages, vis_entity)| {
                    let checked = q_parent
                        .iter_many(vis_usages.into_iter().flatten())
                        .any(|instance_parent| instance_parent.0 == field_entity);
                    let maybe_checked = SceneFunction(move |context, scene| {
                        if checked {
                            let _ = scene.get_or_insert_template::<Checked>(context);
                        }
                    });
                    bsn! {
                        vis_ui_scene(vis_entity, vis_id.clone(), vis_name.cloned())
                        maybe_checked
                    }
                })
                .collect::<Vec<_>>();

            source_ui_scene(
                source_entity,
                source_id.clone(),
                source_name.cloned(),
                vis_ui_scenes,
            )
        })
        .collect::<Vec<_>>();

    bsn! {
        @FeathersListView {
            @rows: {Box::new(source_ui_scenes) as Box<dyn SceneList>},
        }
        Node {
            overflow: Overflow::hidden(),
            flex_grow: 1.0,
        }
    }
}

// ======== UI interaction ========

fn on_host_tab_click(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    q_vis_ui: Query<&VisUiRepresentsEntity>,
    q_parent: Query<&ChildOf>,
    q_children: Query<&Children>,
) {
    let host_tab_container_entity = q_parent.get(click.entity).unwrap().0;
    let inspector_entity = q_parent.get(host_tab_container_entity).unwrap().0;
    let vis_list_entity = q_children.get(inspector_entity).unwrap()[1];

    // Replace the visualization list
    let host_entity = q_vis_ui.get(click.entity).unwrap().0;
    let field_entity = q_vis_ui.get(inspector_entity).unwrap().0;
    commands.entity(vis_list_entity).despawn();
    commands.run_system_cached_with(
        move |In((host_entity, field_entity, inspector_entity)): In<(Entity, Entity, Entity)>,
              world: &mut World| {
            let new_vis_list = world
                .run_system_cached_with(vis_list_for_host, (host_entity, field_entity))
                .unwrap();
            world
                .spawn_scene(bsn! {
                    new_vis_list
                    ChildOf(inspector_entity)
                })
                .unwrap();
        },
        (host_entity, field_entity, inspector_entity),
    );

    // Update the button highlights
    for button_entity in q_children.get(host_tab_container_entity).unwrap() {
        if *button_entity == click.entity {
            commands
                .entity(*button_entity)
                .insert(ButtonVariant::Normal);
        } else {
            commands.entity(*button_entity).insert(ButtonVariant::Plain);
        }
    }
}

fn on_vis_toggled(
    vis_ui_toggled: On<ValueChange<bool>>,
    mut commands: Commands,
    q_vis_ui: Query<&VisUiRepresentsEntity>,
    q_parent: Query<(&ChildOf, Entity)>,
    q_field_inspector: Query<&FieldInspector>,
    q_vis_usages: Query<&VisualizationUsages>,
) {
    let vis_entity = q_vis_ui.get(vis_ui_toggled.source).unwrap().0;

    // Traverse UI hierarchy
    let vis_list_container = q_parent.get(vis_ui_toggled.source).unwrap().0.0;
    let source_ui_entity = q_parent.get(vis_list_container).unwrap().0.0;
    let listview_content_entity = q_parent.get(source_ui_entity).unwrap().0.0;
    let listview_entity = q_parent.get(listview_content_entity).unwrap().0.0;
    let inspector_entity = q_parent.get(listview_entity).unwrap().0.0;
    let field_entity = q_field_inspector.get(inspector_entity).unwrap().0;

    if vis_ui_toggled.value {
        commands.entity(vis_ui_toggled.source).insert(Checked);
        commands.spawn((VisualizationInstance(vis_entity), ChildOf(field_entity)));
    } else {
        commands.entity(vis_ui_toggled.source).remove::<Checked>();
        // Despawn the visualization instance
        q_parent
            .iter_many(q_vis_usages.get(vis_entity).unwrap())
            .for_each(|(instance_parent, instance_entity)| {
                if instance_parent.0 == field_entity {
                    commands.entity(instance_entity).despawn();
                }
            });
    }
}

// ======== New source ======== TODO: Collapsible source sections

fn source_ui_scene(
    source_entity: Entity,
    source_id: VisualizationSourceId,
    source_name: Option<VisualizationSourceName>,
    vis_list: impl SceneList,
) -> impl Scene {
    let label = source_name
        .map(|name| name.0)
        .unwrap_or_else(|| format!("Source {}", source_id.0));
    bsn! {
        #Root
        VisUiRepresentsEntity(source_entity)
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
        }
        Children [
            Node {
                width: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Start,
                column_gap: px(6),
            }
            Children [
                @FeathersDisclosureToggle,
                Text(label) TextFont { font_size: px(14) } Node {padding: UiRect::top(px(3))} TextOfComponent(#Root),
            ],
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
            }
            Children [
                {vis_list}
            ]
        ]
    }
}

fn on_new_vis_source(
    vis_added: On<Add, VisualizationSourceId>,
    mut commands: Commands,
    q_source: Query<(
        &VisualizationSourceId,
        Option<&VisualizationSourceName>,
        &ChildOf,
    )>,
    q_host: Query<&RepresentedByVisUi, With<HostConnection>>,
    (q_parent, q_children): (Query<&ChildOf>, Query<&Children>),
) {
    let (source_id, source_name, host_ref) = q_source.get(vis_added.entity).unwrap();

    let Ok(host_tabs_ref) = q_host.get(host_ref.0) else {
        // No relevant inspector -> Skip
        return;
    };
    for host_tab_ref in host_tabs_ref.iter() {
        // Inspector <- Host tab
        let host_tab_row_entity = q_parent.get(host_tab_ref).unwrap().0;
        let inspector_entity = q_parent.get(host_tab_row_entity).unwrap().0;
        // Inspector -> Listview content (@FeathersListView implementation detail)
        let listview_entity = q_children.get(inspector_entity).unwrap()[1];
        let listview_content_entity = q_children.get(listview_entity).unwrap()[0];
        let new_entry = commands
            .spawn_scene(source_ui_scene(
                vis_added.entity,
                source_id.clone(),
                source_name.cloned(),
                bsn_list![],
            ))
            .id();

        commands
            .entity(listview_content_entity)
            .queue(insert_child_sorted(new_entry));
    }
}

// ======== New visualization ========

fn vis_ui_scene(
    vis_entity: Entity,
    vis_id: VisualizationId,
    vis_name: Option<VisualizationName>,
) -> impl Scene {
    let label = vis_name
        .map(|name| name.0)
        .unwrap_or_else(|| format!("Visualization {}", vis_id.0));
    bsn! {
        #Root
        VisUiRepresentsEntity(vis_entity)
        @FeathersCheckbox {
            @caption: bsn! { Text(label) ThemedText TextOfComponent(#Root) },
        }
        on(on_vis_toggled)
    }
}

fn on_new_vis(
    vis_added: On<Add, VisualizationId>,
    mut commands: Commands,
    q_vis: Query<(&VisualizationId, Option<&VisualizationName>, &ChildOf)>,
    q_source: Query<&RepresentedByVisUi, With<VisualizationSourceId>>,
    q_children: Query<&Children>,
) {
    let (vis_id, vis_name, source_ref) = q_vis.get(vis_added.entity).unwrap();

    // FIXME: If this vis and its source are both added at the same time, it's possible that the observers for both are batched together and the commands for spawning the source ui might not be applied yet, causing this visualization to be lost
    for source_ui_entity in q_source.get(source_ref.0).into_iter().flatten() {
        let vis_list_container = q_children.get(*source_ui_entity).unwrap()[1];
        let new_entry = commands
            .spawn_scene(vis_ui_scene(
                vis_added.entity,
                vis_id.clone(),
                vis_name.cloned(),
            ))
            .id();

        commands
            .entity(vis_list_container)
            .queue(insert_child_sorted(new_entry));
    }
}

// ======== Name updates ========

fn on_source_name_insert(
    name_inserted: On<Insert, VisualizationSourceName>,
    mut commands: Commands,
    q_source: Query<(&VisualizationSourceName, &RepresentedByVisUi)>,
    q_entry: Query<(&ComponentText, &ChildOf)>,
) {
    let Ok((new_name, ui_ref)) = q_source.get(name_inserted.entity) else {
        // Failed either because this source is not shown on any inspector, or because the component was
        // inserted at spawn and the command creating the UI (including this name) has not been applied yet.
        // Skipping the update is correct in both cases.
        return;
    };
    for ui_entity in ui_ref {
        let (label_ref, container_ref) = q_entry.get(*ui_entity).unwrap();
        commands
            .entity(label_ref.0)
            .insert(Text(new_name.0.clone()));

        commands.entity(*ui_entity).remove::<ChildOf>();
        commands
            .entity(container_ref.0)
            .queue(insert_child_sorted(*ui_entity));
    }
}

fn on_vis_name_insert(
    name_inserted: On<Insert, VisualizationName>,
    mut commands: Commands,
    q_vis: Query<(&VisualizationName, &RepresentedByVisUi)>,
    q_entry: Query<(&ComponentText, &ChildOf)>,
) {
    let Ok((new_name, ui_ref)) = q_vis.get(name_inserted.entity) else {
        // Failed either because this visualization is not shown on any inspector, or because the component
        // was inserted at spawn and the command creating the UI (including this name) has not been applied yet.
        // Skipping the update is correct in both cases.
        return;
    };
    for ui_entity in ui_ref {
        let (caption_ref, container_ref) = q_entry.get(*ui_entity).unwrap();
        commands
            .entity(caption_ref.0)
            .insert(Text(new_name.0.clone()));

        commands.entity(*ui_entity).remove::<ChildOf>();
        commands
            .entity(container_ref.0)
            .queue(insert_child_sorted(*ui_entity));
    }
}

// ======== Inactive markings ========

fn on_vis_inactive(
    vis_inactive: On<Add, InactiveVisualization>,
    mut commands: Commands,
    q_vis: Query<&RepresentedByVisUi>,
    q_text_ref: Query<&ComponentText>,
) {
    for ui_entity in q_vis.get(vis_inactive.entity).into_iter().flatten() {
        let label_ref = q_text_ref.get(*ui_entity).unwrap();
        // PropagateOver temporarily disables text color inheritance
        commands
            .entity(label_ref.0)
            .insert(PropagateOver::<TextColor>::default())
            .insert(ThemeTextColor(CHECKBOX_TEXT_DISABLED));
    }
}

fn on_vis_active(
    vis_active: On<Remove, InactiveVisualization>,
    mut commands: Commands,
    q_vis: Query<&RepresentedByVisUi>,
    q_text_ref: Query<&ComponentText>,
) {
    for ui_entity in q_vis.get(vis_active.entity).into_iter().flatten() {
        let label_ref = q_text_ref.get(*ui_entity).unwrap();
        // Removing PropagateOver re-triggers propagation
        commands
            .entity(label_ref.0)
            .try_remove::<ThemeTextColor>()
            .try_remove::<PropagateOver<TextColor>>();
    }
}

/// Inserts a child entity sorted by its [ComponentText].
fn insert_child_sorted(child: Entity) -> impl EntityCommand {
    move |mut parent: EntityWorldMut| -> Result<(), BevyError> {
        let world = parent.world();

        let new_text = world
            .get::<ComponentText>(child)
            .and_then(|t_ref| world.get::<Text>(t_ref.0).map(|t| t.0.to_lowercase()));

        let index = parent
            .get::<Children>()
            .into_iter()
            .flatten()
            .enumerate()
            .find_map(|(i, e)| {
                let this_text = world
                    .get::<ComponentText>(*e)
                    .and_then(|t_ref| world.get::<Text>(t_ref.0).map(|t| t.0.to_lowercase()));
                (new_text < this_text).then_some(i)
            });

        if let Some(i) = index {
            parent.insert_child(i, child);
        } else {
            parent.add_child(child);
        }

        Ok(())
    }
}
