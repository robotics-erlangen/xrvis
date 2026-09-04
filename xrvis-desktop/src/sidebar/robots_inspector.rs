use bevy::ecs::template::{EntityTemplate, TemplateContext};
use bevy::feathers::theme::{ThemeBackgroundColor, ThemeBorderColor};
use bevy::feathers::tokens;
use bevy::prelude::*;

pub fn robots_inspector_plugin(app: &mut App) {}

pub fn scene(field_entity: Entity) -> impl Scene {
    bsn! {
        RobotsInspector { field_entity }
    }
}

#[derive(Component, Clone, Copy)]
#[relationship(relationship_target = FieldInspectedBy)]
struct RobotsInspector(Entity);
#[derive(Component, Clone, Copy)]
#[relationship_target(relationship = RobotsInspector, linked_spawn)]
struct FieldInspectedBy(Entity);

#[derive(Default)]
struct RobotsInspectorTemplate {
    field_entity: EntityTemplate,
}
impl FromTemplate for RobotsInspector {
    type Template = RobotsInspectorTemplate;
}
impl Template for RobotsInspectorTemplate {
    type Output = RobotsInspector;

    fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
        let scene = bsn! {
            Node {
                width: px(300),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                padding: px(6),
                border: {UiRect::right(px(1))},
            }
            ThemeBackgroundColor(tokens::PANE_BODY_BG)
            ThemeBorderColor(tokens::PANE_HEADER_BORDER)
            Children [
            ]
        };

        context.entity.apply_scene(scene)?;
        Ok(RobotsInspector(context.entity.id()))
    }

    fn clone_template(&self) -> Self {
        Self {
            field_entity: self.field_entity,
        }
    }
}
