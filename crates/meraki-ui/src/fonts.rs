use iced::font::{self, Font};

/// Família tipográfica oficial embutida no binário do Meraki (`Inter` v4).
pub const INTER_FAMILY: font::Family = font::Family::Name("Inter");

pub const INTER_REGULAR_BYTES: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
pub const INTER_MEDIUM_BYTES: &[u8] = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
pub const INTER_SEMIBOLD_BYTES: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
pub const INTER_BOLD_BYTES: &[u8] = include_bytes!("../../../assets/fonts/Inter-Bold.ttf");

/// Conjunto completo das 4 variações estáticas da família `Inter` registradas no startup.
pub const EMBEDDED_FONTS: [&[u8]; 4] = [
    INTER_REGULAR_BYTES,
    INTER_MEDIUM_BYTES,
    INTER_SEMIBOLD_BYTES,
    INTER_BOLD_BYTES,
];

pub const FONT_REGULAR: Font = Font {
    family: INTER_FAMILY,
    weight: font::Weight::Normal,
    ..Font::DEFAULT
};

pub const FONT_MEDIUM: Font = Font {
    family: INTER_FAMILY,
    weight: font::Weight::Medium,
    ..Font::DEFAULT
};

pub const FONT_SEMIBOLD: Font = Font {
    family: INTER_FAMILY,
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};

pub const FONT_BOLD: Font = Font {
    family: INTER_FAMILY,
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};

#[cfg(test)]
mod tests {
    use super::*;

    const TTF_MAGIC: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

    #[test]
    fn embedded_inter_fonts_are_valid_distinct_truetype_binaries() {
        for bytes in EMBEDDED_FONTS {
            assert!(bytes.len() > 50_000, "TTF embutida inesperadamente pequena");
            assert_eq!(&bytes[..4], &TTF_MAGIC, "cabeçalho sfnt inválido");
        }
        assert_ne!(INTER_REGULAR_BYTES, INTER_MEDIUM_BYTES);
        assert_ne!(INTER_MEDIUM_BYTES, INTER_SEMIBOLD_BYTES);
        assert_ne!(INTER_SEMIBOLD_BYTES, INTER_BOLD_BYTES);
    }

    #[test]
    fn font_tokens_use_embedded_inter_family_and_explicit_weights() {
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
