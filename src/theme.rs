use eframe::egui::{self, Color32, Stroke, Visuals};

pub const STORAGE_KEY: &str = "video-annotations-theme";

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Ocean,
    Forest,
    Plum,
    Sand,
}

impl Theme {
    pub const ALL: [Self; 6] = [
        Self::Dark,
        Self::Light,
        Self::Ocean,
        Self::Forest,
        Self::Plum,
        Self::Sand,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Ocean => "Ocean",
            Self::Forest => "Forest",
            Self::Plum => "Plum",
            Self::Sand => "Sand",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|theme| theme.label() == id)
            .unwrap_or_default()
    }

    pub fn apply(self, ctx: &egui::Context) {
        let visuals = self.visuals();
        let mode = if visuals.dark_mode {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_visuals_of(mode, visuals);
        ctx.set_theme(mode);
        ctx.request_repaint();
    }

    pub fn visuals(self) -> Visuals {
        let (dark, background, surface, widget, hover, active, accent, text, muted, border) =
            match self {
                Self::Dark => return Visuals::dark(),
                Self::Light => return Visuals::light(),
                Self::Ocean => (
                    true,
                    [15, 27, 42],
                    [22, 39, 57],
                    [32, 54, 74],
                    [42, 77, 102],
                    [35, 97, 130],
                    [80, 200, 230],
                    [230, 244, 250],
                    [159, 187, 204],
                    [69, 98, 119],
                ),
                Self::Forest => (
                    true,
                    [19, 31, 27],
                    [27, 43, 36],
                    [38, 58, 47],
                    [53, 82, 65],
                    [61, 102, 75],
                    [137, 211, 149],
                    [234, 246, 233],
                    [167, 193, 168],
                    [72, 99, 81],
                ),
                Self::Plum => (
                    true,
                    [31, 22, 41],
                    [43, 31, 55],
                    [59, 42, 72],
                    [84, 58, 102],
                    [106, 64, 122],
                    [226, 156, 243],
                    [248, 232, 252],
                    [196, 168, 210],
                    [101, 76, 116],
                ),
                Self::Sand => (
                    false,
                    [247, 239, 223],
                    [255, 249, 237],
                    [234, 221, 194],
                    [226, 204, 161],
                    [214, 181, 126],
                    [143, 88, 22],
                    [53, 41, 28],
                    [109, 89, 61],
                    [181, 157, 117],
                ),
            };
        let rgb = |[r, g, b]: [u8; 3]| Color32::from_rgb(r, g, b);
        let mut visuals = if dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        visuals.panel_fill = rgb(background);
        visuals.window_fill = rgb(surface);
        visuals.window_stroke = Stroke::new(1.0, rgb(border));
        visuals.extreme_bg_color = rgb(background);
        visuals.faint_bg_color = rgb(widget);
        visuals.code_bg_color = rgb(widget);
        visuals.weak_text_color = Some(rgb(muted));
        visuals.hyperlink_color = rgb(accent);
        visuals.selection.bg_fill = rgb(active);
        visuals.selection.stroke = Stroke::new(1.0, rgb(text));
        visuals.text_cursor.stroke.color = rgb(accent);
        for (state, fill) in [
            (&mut visuals.widgets.noninteractive, surface),
            (&mut visuals.widgets.inactive, widget),
            (&mut visuals.widgets.hovered, hover),
            (&mut visuals.widgets.active, active),
            (&mut visuals.widgets.open, active),
        ] {
            state.bg_fill = rgb(fill);
            state.weak_bg_fill = rgb(fill);
            state.bg_stroke = Stroke::new(1.0, rgb(border));
            state.fg_stroke = Stroke::new(1.0, rgb(text));
        }
        visuals.widgets.hovered.bg_stroke.color = rgb(accent);
        visuals.widgets.active.bg_stroke.color = rgb(accent);
        visuals
    }
}
