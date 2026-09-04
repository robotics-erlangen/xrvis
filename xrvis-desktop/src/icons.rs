use bevy::ecs::template::TemplateContext;
use bevy::prelude::*;

#[derive(Resource)]
struct IconFont(Handle<Font>);

pub fn icons_plugin(app: &mut App) {
    let icon_font = Font::from_bytes(lucide_icons::LUCIDE_FONT_BYTES.to_vec());

    let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
    let icon_handle = fonts.add(icon_font);
    app.insert_resource(IconFont(icon_handle));
}

pub fn icon(glyph: lucide_icons::Icon, size: Val) -> impl Scene {
    bsn! {
        Text({glyph.unicode()})
        ~IconFontTemplate(size)
    }
}

#[derive(Default)]
struct IconFontTemplate(FontSize);

impl Template for IconFontTemplate {
    type Output = TextFont;

    fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
        Ok(TextFont {
            font: context.resource::<IconFont>().0.clone().into(),
            font_size: self.0,
            ..default()
        })
    }

    fn clone_template(&self) -> Self {
        Self(self.0)
    }
}
