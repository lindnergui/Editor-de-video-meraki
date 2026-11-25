use iced::theme::Palette;
use iced::widget::{button, container, rule, scrollable, slider, svg, text_input, toggler};
use iced::{color, Background, Border, Color, Theme};

/// Fundo base da janela, viewport e área da timeline (`#111218`).
pub const BG_BASE: Color = color!(0x111218);
/// Fundo dos painéis principais: sidebar, topbar e inspetor (`#181922`).
pub const BG_PANEL: Color = color!(0x181922);
/// Fundo de cards, inputs, trilhas e superfícies elevadas (`#212330`).
pub const BG_SURFACE: Color = color!(0x212330);
/// Bordas e divisores sutis de 1px (`#2A2D3D`).
pub const BORDER_SUBTLE: Color = color!(0x2A2D3D);
/// Cor de destaque oficial: aba ativa, playhead, sliders e botão Exportar (`#735BF2`).
pub const ACCENT: Color = color!(0x735BF2);
/// Destaque de hover e estado secundário interativo (`#8670F5`).
pub const ACCENT_HOVER: Color = color!(0x8670F5);
/// Tom pressionado derivado da cor de destaque (`#624AE0`).
pub const ACCENT_PRESSED: Color = color!(0x624AE0);
/// Texto principal e formas de onda (`#E2E4F0`).
pub const TEXT_PRIMARY: Color = color!(0xE2E4F0);
/// Texto secundário e ícones inativos (`#8B8FA3`).
pub const TEXT_SECONDARY: Color = color!(0x8B8FA3);
/// Texto e ícones sobre superfícies com cor de destaque.
pub const TEXT_ON_ACCENT: Color = Color::WHITE;
/// Cor de estado positivo / pronto (`#3DD68C`).
pub const SUCCESS: Color = color!(0x3DD68C);
/// Cor de atenção / faixa intermediária no medidor dB (`#F5B041`).
pub const WARNING: Color = color!(0xF5B041);
/// Cor de alerta destrutivo / erro (`#F05252`).
pub const DANGER: Color = color!(0xF05252);

/// Raio de arredondamento padrão para painéis e cards (`8px`).
pub const RADIUS_PANEL: f32 = 8.0;
/// Raio de arredondamento padrão para botões e clipes na timeline (`6px`).
pub const RADIUS_CONTROL: f32 = 6.0;
/// Espessura padrão de bordas sutis (`1px`).
pub const BORDER_WIDTH: f32 = 1.0;
/// Altura fixa da barra superior.
pub const TOPBAR_HEIGHT: f32 = 52.0;
/// Largura fixa da barra lateral esquerda de navegação.
pub const SIDEBAR_WIDTH: f32 = 92.0;

pub use crate::fonts::{FONT_BOLD, FONT_MEDIUM, FONT_REGULAR, FONT_SEMIBOLD, INTER_FAMILY};

/// Constrói o tema oficial escuro do Meraki para o `iced`.
pub fn meraki_theme() -> Theme {
    Theme::custom(
        "Meraki Dark".to_string(),
        Palette { background: BG_BASE, text: TEXT_PRIMARY, primary: ACCENT, success: SUCCESS, danger: DANGER },
    )
}

fn border_with(color: Color, width: f32, radius: f32) -> Border {
    Border { color, width, radius: radius.into() }
}

fn container_box(bg: Color, border_color: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(TEXT_PRIMARY),
        border: border_with(border_color, BORDER_WIDTH, radius),
        ..container::Style::default()
    }
}

fn accent_button_style(status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(ACCENT)),
        text_color: TEXT_ON_ACCENT,
        border: border_with(Color::TRANSPARENT, 0.0, RADIUS_CONTROL),
        ..button::Style::default()
    };
    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style { background: Some(Background::Color(ACCENT_HOVER)), ..base },
        button::Status::Pressed => button::Style { background: Some(Background::Color(ACCENT_PRESSED)), ..base },
        button::Status::Disabled => button::Style {
            background: base.background.map(|bg| bg.scale_alpha(0.5)),
            text_color: base.text_color.scale_alpha(0.5),
            ..base
        },
    }
}

pub fn base_canvas(_theme: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(BG_BASE)), text_color: Some(TEXT_PRIMARY), ..container::Style::default() }
}
pub fn topbar(_theme: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(BG_PANEL)), text_color: Some(TEXT_PRIMARY), ..container::Style::default() }
}
pub fn sidebar(_theme: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(BG_PANEL)), text_color: Some(TEXT_PRIMARY), ..container::Style::default() }
}
pub fn panel(_theme: &Theme) -> container::Style { container_box(BG_PANEL, BORDER_SUBTLE, RADIUS_PANEL) }
pub fn card(_theme: &Theme) -> container::Style { container_box(BG_SURFACE, BORDER_SUBTLE, RADIUS_PANEL) }
pub fn badge(_theme: &Theme) -> container::Style { container_box(BG_SURFACE, BORDER_SUBTLE, RADIUS_CONTROL) }
pub fn timeline_clip(_theme: &Theme) -> container::Style { container_box(BG_SURFACE, BORDER_SUBTLE, RADIUS_CONTROL) }
pub fn preview_viewport(_theme: &Theme) -> container::Style { container_box(BG_BASE, BORDER_SUBTLE, RADIUS_PANEL) }
pub fn badge_accent(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ACCENT)),
        text_color: Some(TEXT_ON_ACCENT),
        border: border_with(Color::TRANSPARENT, 0.0, RADIUS_CONTROL),
        ..container::Style::default()
    }
}
pub fn playhead(_theme: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(ACCENT)), ..container::Style::default() }
}
pub fn divider(_theme: &Theme) -> rule::Style {
    rule::Style { color: BORDER_SUBTLE, width: 1, radius: 0.0.into(), fill_mode: rule::FillMode::Full }
}
pub fn export_button(_theme: &Theme, status: button::Status) -> button::Style { accent_button_style(status) }
pub fn primary_button(_theme: &Theme, status: button::Status) -> button::Style { accent_button_style(status) }

pub fn nav_tab_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if active {
            return accent_button_style(status);
        }
        let base = button::Style {
            background: None,
            text_color: TEXT_SECONDARY,
            border: border_with(Color::TRANSPARENT, BORDER_WIDTH, RADIUS_CONTROL),
            ..button::Style::default()
        };
        match status {
            button::Status::Active => base,
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(Background::Color(BG_SURFACE)),
                text_color: TEXT_PRIMARY,
                border: border_with(BORDER_SUBTLE, BORDER_WIDTH, RADIUS_CONTROL),
                ..base
            },
            button::Status::Disabled => button::Style { text_color: base.text_color.scale_alpha(0.5), ..base },
        }
    }
}

pub fn secondary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(BG_SURFACE)),
        text_color: TEXT_PRIMARY,
        border: border_with(BORDER_SUBTLE, BORDER_WIDTH, RADIUS_CONTROL),
        ..button::Style::default()
    };
    match status {
        button::Status::Active => base,
        button::Status::Hovered | button::Status::Pressed => {
            button::Style { border: border_with(ACCENT_HOVER, BORDER_WIDTH, RADIUS_CONTROL), ..base }
        }
        button::Status::Disabled => button::Style {
            background: base.background.map(|bg| bg.scale_alpha(0.5)),
            text_color: base.text_color.scale_alpha(0.5),
            ..base
        },
    }
}

pub fn ghost_button(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: None,
        text_color: TEXT_SECONDARY,
        border: border_with(Color::TRANSPARENT, BORDER_WIDTH, RADIUS_CONTROL),
        ..button::Style::default()
    };
    match status {
        button::Status::Active => base,
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(BG_SURFACE)),
            text_color: TEXT_PRIMARY,
            border: border_with(BORDER_SUBTLE, BORDER_WIDTH, RADIUS_CONTROL),
            ..base
        },
        button::Status::Disabled => button::Style { text_color: base.text_color.scale_alpha(0.5), ..base },
    }
}

pub fn folder_item_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if active {
            return button::Style {
                background: Some(Background::Color(BG_SURFACE)),
                text_color: TEXT_PRIMARY,
                border: border_with(ACCENT, BORDER_WIDTH, RADIUS_CONTROL),
                ..button::Style::default()
            };
        }
        let base = button::Style {
            background: None,
            text_color: TEXT_SECONDARY,
            border: border_with(Color::TRANSPARENT, BORDER_WIDTH, RADIUS_CONTROL),
            ..button::Style::default()
        };
        match status {
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(Background::Color(BG_SURFACE)),
                text_color: TEXT_PRIMARY,
                border: border_with(BORDER_SUBTLE, BORDER_WIDTH, RADIUS_CONTROL),
                ..base
            },
            _ => base,
        }
    }
}

pub fn filter_pill_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        if active {
            accent_button_style(status)
        } else {
            let s = secondary_button(theme, status);
            button::Style {
                text_color: if matches!(status, button::Status::Active) { TEXT_SECONDARY } else { TEXT_PRIMARY },
                ..s
            }
        }
    }
}

pub fn media_card_button(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (border_color, border_width) = if selected {
            (ACCENT, 2.0)
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            (ACCENT_HOVER, BORDER_WIDTH)
        } else {
            (BORDER_SUBTLE, BORDER_WIDTH)
        };
        button::Style {
            background: Some(Background::Color(BG_SURFACE)),
            text_color: TEXT_PRIMARY,
            border: border_with(border_color, border_width, RADIUS_PANEL),
            ..button::Style::default()
        }
    }
}

pub fn text_input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused => ACCENT,
        text_input::Status::Hovered => ACCENT_HOVER,
        text_input::Status::Active | text_input::Status::Disabled => BORDER_SUBTLE,
    };
    text_input::Style {
        background: Background::Color(BG_SURFACE),
        border: border_with(border_color, BORDER_WIDTH, RADIUS_CONTROL),
        icon: TEXT_SECONDARY,
        placeholder: TEXT_SECONDARY,
        value: TEXT_PRIMARY,
        selection: ACCENT.scale_alpha(0.4),
    }
}

pub fn preview_slider(_theme: &Theme, status: slider::Status) -> slider::Style {
    let fill = match status {
        slider::Status::Hovered | slider::Status::Dragged => ACCENT_HOVER,
        slider::Status::Active => ACCENT,
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (Background::Color(fill), Background::Color(BG_SURFACE)),
            width: 4.0,
            border: border_with(Color::TRANSPARENT, 0.0, 2.0),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 6.0 },
            background: Background::Color(fill),
            border_width: 1.0,
            border_color: TEXT_ON_ACCENT,
        },
    }
}

pub fn dark_scrollable(_theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let scroller_color = match status {
        scrollable::Status::Active => BORDER_SUBTLE,
        scrollable::Status::Hovered { .. } => ACCENT_HOVER,
        scrollable::Status::Dragged { .. } => ACCENT,
    };
    let rail = scrollable::Rail {
        background: Some(Background::Color(BG_PANEL)),
        border: border_with(Color::TRANSPARENT, 0.0, 4.0),
        scroller: scrollable::Scroller { color: scroller_color, border: border_with(Color::TRANSPARENT, 0.0, 4.0) },
    };
    scrollable::Style { container: container::Style::default(), vertical_rail: rail, horizontal_rail: rail, gap: None }
}

/// Aplica uma cor sólida monocromática a ícones SVG.
pub fn svg_color(color: Color) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |_theme, _status| svg::Style { color: Some(color) }
}

pub fn accent_toggler(_theme: &Theme, status: toggler::Status) -> toggler::Style {
    match status {
        toggler::Status::Active { is_toggled } => toggler::Style {
            background: if is_toggled { ACCENT } else { BG_SURFACE },
            background_border_width: if is_toggled { 0.0 } else { BORDER_WIDTH },
            background_border_color: if is_toggled { Color::TRANSPARENT } else { BORDER_SUBTLE },
            foreground: if is_toggled { TEXT_ON_ACCENT } else { TEXT_SECONDARY },
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
        },
        toggler::Status::Hovered { is_toggled } => toggler::Style {
            background: if is_toggled { ACCENT_HOVER } else { BG_SURFACE },
            background_border_width: BORDER_WIDTH,
            background_border_color: ACCENT_HOVER,
            foreground: if is_toggled { TEXT_ON_ACCENT } else { TEXT_PRIMARY },
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
        },
        toggler::Status::Disabled => toggler::Style {
            background: BG_SURFACE.scale_alpha(0.5),
            background_border_width: BORDER_WIDTH,
            background_border_color: BORDER_SUBTLE.scale_alpha(0.5),
            foreground: TEXT_SECONDARY.scale_alpha(0.5),
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
        },
    }
}

pub fn mixer_toggle_button(
    active: bool,
    active_color: Color,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if active {
            return button::Style {
                background: Some(Background::Color(active_color)),
                text_color: Color::BLACK,
                border: border_with(Color::TRANSPARENT, 0.0, RADIUS_CONTROL),
                ..button::Style::default()
            };
        }
        let border_color = match status {
            button::Status::Hovered | button::Status::Pressed => ACCENT_HOVER,
            _ => BORDER_SUBTLE,
        };
        button::Style {
            background: Some(Background::Color(BG_SURFACE)),
            text_color: TEXT_SECONDARY,
            border: border_with(border_color, BORDER_WIDTH, RADIUS_CONTROL),
            ..button::Style::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::font::{self, Font};

    fn to_rgb_hex(c: Color) -> (u8, u8, u8) {
        ((c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8)
    }

    #[test]
    fn official_palette_matches_design_system() {
        assert_eq!(to_rgb_hex(BG_BASE), (0x11, 0x12, 0x18));
        assert_eq!(to_rgb_hex(BG_PANEL), (0x18, 0x19, 0x22));
        assert_eq!(to_rgb_hex(BG_SURFACE), (0x21, 0x23, 0x30));
        assert_eq!(to_rgb_hex(BORDER_SUBTLE), (0x2A, 0x2D, 0x3D));
        assert_eq!(to_rgb_hex(ACCENT), (0x73, 0x5B, 0xF2));
        assert_eq!(to_rgb_hex(ACCENT_HOVER), (0x86, 0x70, 0xF5));
        assert_eq!(to_rgb_hex(TEXT_PRIMARY), (0xE2, 0xE4, 0xF0));
        assert_eq!(to_rgb_hex(TEXT_SECONDARY), (0x8B, 0x8F, 0xA3));
    }

    #[test]
    fn geometry_tokens_match_design_system() {
        assert!((RADIUS_PANEL - 8.0).abs() < f32::EPSILON);
        assert!((RADIUS_CONTROL - 6.0).abs() < f32::EPSILON);
        assert!((BORDER_WIDTH - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn meraki_theme_uses_official_palette() {
        let palette = meraki_theme().palette();
        assert_eq!(palette.background, BG_BASE);
        assert_eq!(palette.text, TEXT_PRIMARY);
        assert_eq!(palette.primary, ACCENT);
    }

    #[test]
    fn font_tokens_never_pair_weight_medium_with_system_default_family() {
        for token in [FONT_REGULAR, FONT_MEDIUM, FONT_SEMIBOLD, FONT_BOLD] {
            assert_eq!(token.family, INTER_FAMILY);
            assert_ne!(token.family, Font::DEFAULT.family);
        }
        assert_eq!(FONT_REGULAR.weight, font::Weight::Normal);
        assert_eq!(FONT_MEDIUM.weight, font::Weight::Medium);
        assert_eq!(FONT_SEMIBOLD.weight, font::Weight::Semibold);
        assert_eq!(FONT_BOLD.weight, font::Weight::Bold);
    }
}
