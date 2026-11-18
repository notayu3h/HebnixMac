//! themes: built-in Dark/Light plus user themes from themes/ (toml palettes
//! over a dark or light base).

use std::path::Path;

use eframe::egui::{self, Color32};
use serde::Deserialize;

mod presentation;
pub use presentation::*;

/// a user theme file, see examples/themes/ for the format
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeFile {
    /// Absent/1 keeps the original renderer. New presentation is opt-in only.
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub layout: ThemeLayout,
    #[serde(default)]
    pub typography: ThemeTypography,
    #[serde(default)]
    pub components: ThemeComponents,
    #[serde(default)]
    pub pages: ThemePages,
    #[serde(default)]
    pub assets: ThemeAssets,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub colors: ThemeColors,
    /// optional font name. matches a filename in fonts/, not case sensitive.
    /// leave blank to use the default font.
    #[serde(default)]
    pub font: Option<String>,
    /// Optional global UI behaviour and sizing overrides.
    #[serde(default)]
    pub style: ThemeStyle,
    /// Optional per-interaction-state widget styling.
    #[serde(default)]
    pub widgets: ThemeWidgets,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeStyle {
    pub window_corner_radius: Option<u8>,
    pub menu_corner_radius: Option<u8>,
    pub widget_corner_radius: Option<u8>,
    pub window_stroke: Option<String>,
    pub window_stroke_width: Option<f32>,
    pub selection_stroke: Option<String>,
    pub selection_stroke_width: Option<f32>,
    pub item_spacing_x: Option<f32>,
    pub item_spacing_y: Option<f32>,
    pub button_padding_x: Option<f32>,
    pub button_padding_y: Option<f32>,
    pub indent: Option<f32>,
    pub interact_height: Option<f32>,
    pub slider_width: Option<f32>,
    pub text_edit_width: Option<f32>,
    pub animation_time: Option<f32>,
    pub button_frame: Option<bool>,
    pub collapsing_header_frame: Option<bool>,
    pub indent_has_left_vline: Option<bool>,
    pub striped: Option<bool>,
    pub slider_trailing_fill: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeWidgets {
    #[serde(default)]
    pub noninteractive: ThemeWidgetState,
    #[serde(default)]
    pub inactive: ThemeWidgetState,
    #[serde(default)]
    pub hovered: ThemeWidgetState,
    #[serde(default)]
    pub active: ThemeWidgetState,
    #[serde(default)]
    pub open: ThemeWidgetState,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeWidgetState {
    pub bg_fill: Option<String>,
    pub weak_bg_fill: Option<String>,
    pub bg_stroke: Option<String>,
    pub bg_stroke_width: Option<f32>,
    pub fg_stroke: Option<String>,
    pub fg_stroke_width: Option<f32>,
    pub corner_radius: Option<u8>,
    pub expansion: Option<f32>,
}

/// look for a .ttf or .otf in fonts_dir with this name
pub fn find_font_file(fonts_dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    for ext in ["ttf", "otf"] {
        let candidate = fonts_dir.join(format!("{name}.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let entries = std::fs::read_dir(fonts_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_font = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
            .unwrap_or(false);
        if !is_font {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            // exact stem, or a family name followed by a variant suffix:
            // "Lexend" matches "Lexend-VariableFont_wght.ttf"
            let (stem, name_lc) = (stem.to_ascii_lowercase(), name.to_ascii_lowercase());
            let variant = stem
                .strip_prefix(name_lc.as_str())
                .is_some_and(|rest| matches!(rest.chars().next(), Some('-' | '_' | ' ')));
            if stem == name_lc || variant {
                return Some(path);
            }
        }
    }
    None
}

/// stems of the .ttf/.otf files in fonts_dir, sorted (Linux overlay font picker)
#[cfg(target_os = "linux")]
pub fn list_fonts(fonts_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fonts_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
        })
        .filter_map(|path| path.file_stem().and_then(|s| s.to_str()).map(str::to_owned))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

/// switches the app's font to whatever's in fonts_dir. falls back to the
/// default font if nothing's set or the file isn't there.
pub fn apply_font(ctx: &egui::Context, fonts_dir: &Path, font_name: Option<&str>) {
    ctx.set_fonts(theme_fonts(fonts_dir, font_name, false));
}

#[cfg(test)]
mod bundled_font_tests {
    use super::*;
    #[test]
    fn bundled_font_is_opt_in_and_legacy_missing_fonts_keep_defaults() {
        let missing = Path::new("this-directory-does-not-exist");
        let modern = theme_fonts(missing, Some("Barlow-Medium"), true);
        assert_eq!(
            modern.families[&egui::FontFamily::Proportional][0],
            "Barlow-Medium"
        );
        assert_eq!(
            modern.font_data["Barlow-Medium"].font.as_ref(),
            include_bytes!("../../../examples/fonts/Barlow-Medium.ttf")
        );
        assert!(
            !theme_fonts(missing, Some("Barlow-Medium"), false)
                .font_data
                .contains_key("Barlow-Medium")
        );
        assert!(
            !theme_fonts(missing, None, true)
                .font_data
                .contains_key("Barlow-Medium")
        );
    }
}

fn theme_fonts(fonts_dir: &Path, font_name: Option<&str>, bundled: bool) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();

    if let Some(name) = font_name {
        let data = find_font_file(fonts_dir, name)
            .and_then(|path| std::fs::read(path).ok())
            .map(egui::FontData::from_owned)
            .or_else(|| {
                (bundled && name.eq_ignore_ascii_case("Barlow-Medium")).then(|| {
                    egui::FontData::from_static(include_bytes!(
                        "../../../examples/fonts/Barlow-Medium.ttf"
                    ))
                })
            });
        if let Some(data) = data {
            fonts.font_data.insert(name.to_owned(), data.into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, name.to_owned());
        }
    }

    // The platform's own font first: Ubuntu in a Mac window is what makes a
    // ported app feel foreign.
    if font_name.is_none()
        && let Some(data) = native_font_data()
    {
        fonts.font_data.insert("system".to_owned(), data.into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "system".to_owned());
    }

    // glyph fallbacks for the active language go last, behind the theme font
    crate::i18n::fonts::add_fallbacks(&mut fonts);

    fonts
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeColors {
    pub accent: Option<String>,
    pub window_bg: Option<String>,
    pub panel_bg: Option<String>,
    pub widget_bg: Option<String>,
    pub extreme_bg: Option<String>,
    pub faint_bg: Option<String>,
    pub code_bg: Option<String>,
    pub text_edit_bg: Option<String>,
    pub text: Option<String>,
    pub weak_text: Option<String>,
    pub selection_text: Option<String>,
    pub hyperlink: Option<String>,
    pub warn: Option<String>,
    pub error: Option<String>,
}

fn apply_widget_state(target: &mut egui::style::WidgetVisuals, source: &ThemeWidgetState) {
    if let Some(color) = source.bg_fill.as_deref().and_then(parse_color) {
        target.bg_fill = color;
    }
    if let Some(color) = source.weak_bg_fill.as_deref().and_then(parse_color) {
        target.weak_bg_fill = color;
    }
    if let Some(color) = source.bg_stroke.as_deref().and_then(parse_color) {
        target.bg_stroke.color = color;
    }
    if let Some(width) = source.bg_stroke_width {
        target.bg_stroke.width = width.max(0.0);
    }
    if let Some(color) = source.fg_stroke.as_deref().and_then(parse_color) {
        target.fg_stroke.color = color;
    }
    if let Some(width) = source.fg_stroke_width {
        target.fg_stroke.width = width.max(0.0);
    }
    if let Some(radius) = source.corner_radius {
        target.corner_radius = egui::CornerRadius::same(radius);
    }
    if let Some(expansion) = source.expansion {
        target.expansion = expansion.max(0.0);
    }
}

fn parse_color(s: &str) -> Option<Color32> {
    let s = s.trim().trim_start_matches('#');
    if !s.is_ascii() {
        return None;
    }
    let bytes = match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            let a = u8::from_str_radix(&s[6..8], 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };
    Some(Color32::from_rgba_unmultiplied(
        bytes.0, bytes.1, bytes.2, bytes.3,
    ))
}

/// A lower-glare light palette. This remains recognisably light, but avoids
/// egui's near-white default surfaces and gives panels/widgets more depth.
fn muted_light_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::light();
    visuals.window_fill = Color32::from_rgb(0xd8, 0xda, 0xdd);
    visuals.panel_fill = Color32::from_rgb(0xcf, 0xd2, 0xd6);
    visuals.extreme_bg_color = Color32::from_rgb(0xc2, 0xc6, 0xcb);
    visuals.faint_bg_color = Color32::from_rgb(0xc9, 0xcc, 0xd1);
    visuals.code_bg_color = Color32::from_rgb(0xc4, 0xc8, 0xcd);
    visuals.text_edit_bg_color = Some(Color32::from_rgb(0xc7, 0xca, 0xcf));

    let widget_bg = Color32::from_rgb(0xc5, 0xc9, 0xce);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.open,
    ] {
        widget.bg_fill = widget_bg;
        widget.weak_bg_fill = widget_bg;
    }
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0xb6, 0xc5, 0xd4);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xb6, 0xc5, 0xd4);
    visuals.widgets.active.bg_fill = Color32::from_rgb(0x9f, 0xb6, 0xca);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0x9f, 0xb6, 0xca);
    visuals
}

/// scale alpha on every bg fill to make the window translucent. call after
/// apply_theme (its reset stops this compounding). needs the viewport made
/// with_transparent(true).
pub fn apply_window_opacity(ctx: &egui::Context, opacity: f32) {
    let o = opacity.clamp(0.5, 1.0);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("hebnix.theme.opacity"), o));
    if o >= 0.995 {
        return;
    }
    let scale = |c: Color32| -> Color32 {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (c.a() as f32 * o) as u8)
    };
    let theme = ctx.theme();
    ctx.style_mut_of(theme, |style| {
        let v = &mut style.visuals;
        v.window_fill = scale(v.window_fill);
        v.panel_fill = scale(v.panel_fill);
        v.extreme_bg_color = scale(v.extreme_bg_color);
        v.faint_bg_color = scale(v.faint_bg_color);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.open,
        ] {
            w.bg_fill = scale(w.bg_fill);
            w.weak_bg_fill = scale(w.weak_bg_fill);
        }
    });
}

/// theme names: built-ins + themes/*.toml stems
pub fn list_themes(themes_dir: &Path) -> Vec<String> {
    let mut names = vec![
        "Dark".to_string(),
        "Light".to_string(),
        CLASSIC_THEME.to_string(),
    ];
    if let Ok(entries) = std::fs::read_dir(themes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names
}

/// apply a theme by name. Errs (for the caller to log) if the file is
/// missing/invalid, caller falls back to Dark.
/// Flat, macOS-flavoured palette. No gradients: every surface is one colour
/// with a one-pixel border.
/// Name of the built-in theme that restores the original Hebnix look.
pub const CLASSIC_THEME: &str = "Hebnix Classic";

pub const ACCENT: Color32 = Color32::from_rgb(0x5B, 0x7C, 0xFF);
const SURFACE: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);
const PANEL: Color32 = Color32::from_rgb(0x15, 0x18, 0x1D);
const CARD: Color32 = Color32::from_rgb(0x18, 0x1C, 0x22);
const SUNKEN: Color32 = Color32::from_rgb(0x0E, 0x10, 0x14);
const BORDER: Color32 = Color32::from_rgb(0x26, 0x2B, 0x34);
const TEXT: Color32 = Color32::from_rgb(0xE6, 0xEA, 0xF2);
const WEAK_TEXT: Color32 = Color32::from_rgb(0x93, 0x9B, 0xAE);
const CONTROL: Color32 = Color32::from_rgb(0x1B, 0x1F, 0x26);
const CONTROL_HOVER: Color32 = Color32::from_rgb(0x21, 0x26, 0x30);
const CONTROL_ACTIVE: Color32 = Color32::from_rgb(0x27, 0x2D, 0x39);

fn native_font_data() -> Option<egui::FontData> {
    #[cfg(target_os = "macos")]
    {
        for path in [
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                return Some(egui::FontData::from_owned(bytes));
            }
        }
    }
    None
}

fn widget_visuals(
    bg_fill: Color32,
    stroke: Color32,
    fg: Color32,
) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill,
        weak_bg_fill: bg_fill,
        bg_stroke: egui::Stroke::new(1.0, stroke),
        corner_radius: egui::CornerRadius::same(7),
        fg_stroke: egui::Stroke::new(1.0, fg),
        expansion: 0.0,
    }
}

fn native_dark_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = SURFACE;
    visuals.window_fill = PANEL;
    visuals.faint_bg_color = CARD;
    visuals.extreme_bg_color = SUNKEN;
    visuals.code_bg_color = SUNKEN;
    visuals.window_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = egui::CornerRadius::same(10);
    visuals.menu_corner_radius = egui::CornerRadius::same(8);
    visuals.override_text_color = None;
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = Color32::from_rgb(0xE0, 0xA6, 0x4B);
    visuals.error_fg_color = Color32::from_rgb(0xE5, 0x64, 0x5C);
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.noninteractive = widget_visuals(Color32::TRANSPARENT, BORDER, WEAK_TEXT);
    visuals.widgets.inactive = widget_visuals(CONTROL, BORDER, TEXT);
    visuals.widgets.hovered = widget_visuals(CONTROL_HOVER, Color32::from_rgb(0x33, 0x3B, 0x49), TEXT);
    visuals.widgets.active = widget_visuals(CONTROL_ACTIVE, ACCENT, Color32::WHITE);
    visuals.widgets.open = widget_visuals(CONTROL_ACTIVE, BORDER, TEXT);
    visuals
}

fn native_metrics(style: &mut egui::Style) {
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.interact_size = egui::vec2(40.0, 30.0);
    style.spacing.indent = 16.0;
    style.spacing.slider_width = 140.0;
    style.spacing.window_margin = egui::Margin::same(12);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.scroll.bar_width = 10.0;
    style.spacing.scroll.floating = false;
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::proportional(19.0),
        ),
        (egui::TextStyle::Body, egui::FontId::proportional(13.5)),
        (egui::TextStyle::Button, egui::FontId::proportional(13.5)),
        (egui::TextStyle::Small, egui::FontId::proportional(11.5)),
        (egui::TextStyle::Monospace, egui::FontId::monospace(12.0)),
    ]
    .into();
}

/// One card: flat fill, hairline border, even inner padding.
pub fn card_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::group(style)
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(14))
}

/// Accent-filled action. Shape and spacing still come from the style, so it
/// matches every other control.
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(Color32::WHITE))
            .fill(ACCENT)
            .stroke(egui::Stroke::NONE),
    )
}

pub fn apply_theme(
    ctx: &egui::Context,
    themes_dir: &Path,
    fonts_dir: &Path,
    name: &str,
) -> Result<(), String> {
    // Validate before touching the context, so a failed theme cannot half-apply.
    let loaded = if matches!(name, "Dark" | "Light" | CLASSIC_THEME) {
        None
    } else {
        let text = std::fs::read_to_string(themes_dir.join(format!("{name}.toml")))
            .map_err(|e| format!("Theme file '{name}.toml' is missing: {e}"))?;
        Some(parse_theme(&text)?)
    };
    prepare_presentation(ctx, loaded.as_ref(), themes_dir)?;
    match name {
        "Dark" => {
            ctx.set_theme(egui::Theme::Dark);
            ctx.set_visuals_of(egui::Theme::Dark, native_dark_visuals());
            ctx.style_mut_of(egui::Theme::Dark, native_metrics);
            apply_font(ctx, fonts_dir, None);
            Ok(())
        }
        "Light" => {
            ctx.set_theme(egui::Theme::Light);
            ctx.set_visuals_of(egui::Theme::Light, muted_light_visuals());
            apply_font(ctx, fonts_dir, None);
            Ok(())
        }
        // The look Hebnix shipped before the macOS restyle: stock egui dark,
        // stock spacing, icon-free navigation in its own blue.
        CLASSIC_THEME => {
            ctx.set_theme(egui::Theme::Dark);
            ctx.set_style_of(egui::Theme::Dark, egui::Style::default());
            ctx.set_visuals_of(egui::Theme::Dark, egui::Visuals::dark());
            use_classic_presentation(ctx);
            apply_font(ctx, fonts_dir, None);
            Ok(())
        }
        _ => {
            let theme = loaded.as_ref().expect("user theme was loaded above");

            let (base_theme, mut visuals) = if theme.base.eq_ignore_ascii_case("light") {
                (egui::Theme::Light, muted_light_visuals())
            } else {
                (egui::Theme::Dark, egui::Visuals::dark())
            };

            let c = &theme.colors;
            if let Some(col) = c.window_bg.as_deref().and_then(parse_color) {
                visuals.window_fill = col;
                visuals.extreme_bg_color = col.gamma_multiply(0.7);
            }
            if let Some(col) = c.panel_bg.as_deref().and_then(parse_color) {
                visuals.panel_fill = col;
                visuals.faint_bg_color = col.gamma_multiply(1.2);
            }
            if let Some(col) = c.widget_bg.as_deref().and_then(parse_color) {
                visuals.widgets.inactive.bg_fill = col;
                visuals.widgets.inactive.weak_bg_fill = col;
                visuals.widgets.noninteractive.bg_fill = col;
                visuals.widgets.noninteractive.weak_bg_fill = col;
                visuals.widgets.open.bg_fill = col;
                visuals.widgets.open.weak_bg_fill = col;
            }
            if let Some(col) = c.extreme_bg.as_deref().and_then(parse_color) {
                visuals.extreme_bg_color = col;
            }
            if let Some(col) = c.faint_bg.as_deref().and_then(parse_color) {
                visuals.faint_bg_color = col;
            }
            if let Some(col) = c.code_bg.as_deref().and_then(parse_color) {
                visuals.code_bg_color = col;
            }
            if let Some(col) = c.text_edit_bg.as_deref().and_then(parse_color) {
                visuals.text_edit_bg_color = Some(col);
            }
            if let Some(col) = c.accent.as_deref().and_then(parse_color) {
                visuals.selection.bg_fill = col;
                visuals.hyperlink_color = col;
                visuals.widgets.hovered.bg_fill = col.gamma_multiply(0.85);
                visuals.widgets.hovered.weak_bg_fill = col.gamma_multiply(0.85);
                visuals.widgets.active.bg_fill = col;
                visuals.widgets.active.weak_bg_fill = col;
            }
            if let Some(col) = c.text.as_deref().and_then(parse_color) {
                visuals.override_text_color = Some(col);
            }
            if let Some(col) = c.weak_text.as_deref().and_then(parse_color) {
                visuals.weak_text_color = Some(col);
            }
            if let Some(col) = c.selection_text.as_deref().and_then(parse_color) {
                visuals.selection.stroke.color = col;
            }
            if let Some(col) = c.hyperlink.as_deref().and_then(parse_color) {
                visuals.hyperlink_color = col;
            }
            if let Some(col) = c.warn.as_deref().and_then(parse_color) {
                visuals.warn_fg_color = col;
            }
            if let Some(col) = c.error.as_deref().and_then(parse_color) {
                visuals.error_fg_color = col;
            }

            let s = &theme.style;
            if let Some(radius) = s.window_corner_radius {
                visuals.window_corner_radius = egui::CornerRadius::same(radius);
            }
            if let Some(radius) = s.menu_corner_radius {
                visuals.menu_corner_radius = egui::CornerRadius::same(radius);
            }
            if let Some(radius) = s.widget_corner_radius {
                for widget in [
                    &mut visuals.widgets.noninteractive,
                    &mut visuals.widgets.inactive,
                    &mut visuals.widgets.hovered,
                    &mut visuals.widgets.active,
                    &mut visuals.widgets.open,
                ] {
                    widget.corner_radius = egui::CornerRadius::same(radius);
                }
            }
            if let Some(col) = s.window_stroke.as_deref().and_then(parse_color) {
                visuals.window_stroke.color = col;
            }
            if let Some(width) = s.window_stroke_width {
                visuals.window_stroke.width = width.max(0.0);
            }
            if let Some(col) = s.selection_stroke.as_deref().and_then(parse_color) {
                visuals.selection.stroke.color = col;
            }
            if let Some(width) = s.selection_stroke_width {
                visuals.selection.stroke.width = width.max(0.0);
            }
            if let Some(value) = s.button_frame {
                visuals.button_frame = value;
            }
            if let Some(value) = s.collapsing_header_frame {
                visuals.collapsing_header_frame = value;
            }
            if let Some(value) = s.indent_has_left_vline {
                visuals.indent_has_left_vline = value;
            }
            if let Some(value) = s.striped {
                visuals.striped = value;
            }
            if let Some(value) = s.slider_trailing_fill {
                visuals.slider_trailing_fill = value;
            }

            apply_widget_state(
                &mut visuals.widgets.noninteractive,
                &theme.widgets.noninteractive,
            );
            apply_widget_state(&mut visuals.widgets.inactive, &theme.widgets.inactive);
            apply_widget_state(&mut visuals.widgets.hovered, &theme.widgets.hovered);
            apply_widget_state(&mut visuals.widgets.active, &theme.widgets.active);
            apply_widget_state(&mut visuals.widgets.open, &theme.widgets.open);

            ctx.set_theme(base_theme);
            ctx.set_visuals_of(base_theme, visuals);
            ctx.style_mut_of(base_theme, |style| {
                if let Some(value) = s.item_spacing_x {
                    style.spacing.item_spacing.x = value.max(0.0);
                }
                if let Some(value) = s.item_spacing_y {
                    style.spacing.item_spacing.y = value.max(0.0);
                }
                if let Some(value) = s.button_padding_x {
                    style.spacing.button_padding.x = value.max(0.0);
                }
                if let Some(value) = s.button_padding_y {
                    style.spacing.button_padding.y = value.max(0.0);
                }
                if let Some(value) = s.indent {
                    style.spacing.indent = value.max(0.0);
                }
                if let Some(value) = s.interact_height {
                    style.spacing.interact_size.y = value.max(0.0);
                }
                if let Some(value) = s.slider_width {
                    style.spacing.slider_width = value.max(0.0);
                }
                if let Some(value) = s.text_edit_width {
                    style.spacing.text_edit_width = value.max(0.0);
                }
                if let Some(value) = s.animation_time {
                    style.animation_time = value.max(0.0);
                }
            });
            ctx.set_fonts(theme_fonts(
                fonts_dir,
                theme.font.as_deref(),
                theme.schema_version == Some(2),
            ));
            apply_presentation_style(ctx, theme);
            Ok(())
        }
    }
}

    #[test]
    fn classic_theme_brings_back_the_original_look() {
        let ctx = egui::Context::default();
        let here = Path::new(".");
        apply_theme(&ctx, here, here, CLASSIC_THEME).expect("classic theme applies");
        assert!(list_themes(here).contains(&CLASSIC_THEME.to_string()));

        let shipped = egui::Visuals::dark();
        let applied = ctx.style_of(egui::Theme::Dark).visuals.clone();
        assert_eq!(applied.panel_fill, shipped.panel_fill);
        assert_eq!(
            applied.widgets.inactive.bg_fill,
            shipped.widgets.inactive.bg_fill
        );

        let presentation = presentation(&ctx).expect("classic presentation");
        assert!(!presentation.components.navigation.icons);
        assert_eq!(presentation.components.navigation.height, 34.0);
        assert_eq!(presentation.components.navigation.selected, "#2E86C1");
    }
