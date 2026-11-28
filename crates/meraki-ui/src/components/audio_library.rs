use crate::components::effects_library::matches_query;
use crate::components::timeline_math::format_ruler_label;
use crate::components::tooltip::icon_tooltip;
use crate::i18n;
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::tooltip::Position;
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input, Column,
};
use iced::{Element, Length};
use meraki_core::{
    AppLanguage, AssetId, AudioEffectKind, AudioPreset, MediaAsset, MediaKind, Project,
    TrackAudioSettings, VoicePreset,
};

/// Mensagens emitidas pelo painel esquerdo da aba Áudio.
#[derive(Debug, Clone)]
pub enum AudioLibraryMessage {
    SearchChanged(String),
    ImportAudioClicked,
    AddAudioToSelectedTrack(AssetId),
    ApplyAudioPreset(AudioPreset),
    ApplyVoicePreset(VoicePreset),
    ToggleAudioEffect(AudioEffectKind),
}

/// Painel esquerdo (`240px`, `#181922`): Biblioteca de Áudio, Presets de Voz e Efeitos de Áudio.
pub fn view<'a>(
    project: &'a Project,
    search_query: &'a str,
    selected_track_name: &'a str,
    settings: &TrackAudioSettings,
    lang: AppLanguage,
) -> Element<'a, AudioLibraryMessage> {
    let s = i18n::strings(lang);
    let header = text(s.audio_library_title)
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY)
        .wrapping(text::Wrapping::None);

    let search = row![
        icons::icon(icons::SEARCH, 13.0, theme::TEXT_SECONDARY),
        text_input(s.audio_search_placeholder, search_query)
            .on_input(AudioLibraryMessage::SearchChanged)
            .size(12)
            .padding([5, 8])
            .style(theme::text_input_style),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let assets_header = row![
        text(s.audio_music_sfx)
            .size(12)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_PRIMARY),
        horizontal_space(),
        button(
            row![
                icons::icon(icons::PLUS, 11.0, theme::TEXT_PRIMARY),
                text(s.audio_import_short)
                    .size(10)
                    .font(theme::FONT_SEMIBOLD)
                    .color(theme::TEXT_PRIMARY)
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(4)
            .align_y(Vertical::Center),
        )
        .padding([4, 7])
        .style(theme::secondary_button)
        .on_press(AudioLibraryMessage::ImportAudioClicked),
    ]
    .align_y(Vertical::Center);

    let audio_assets: Vec<&MediaAsset> = project
        .assets
        .iter()
        .filter(|asset| asset.kind == MediaKind::Audio)
        .filter(|asset| matches_query(search_query, &asset.name, "audio musica music sfx trilha"))
        .collect();

    let mut list_col = Column::new().spacing(6).width(Length::Fill);
    if audio_assets.is_empty() {
        list_col = list_col.push(
            container(text(s.audio_empty).size(11).color(theme::TEXT_SECONDARY))
                .width(Length::Fill)
                .padding(10)
                .style(theme::card),
        );
    } else {
        for asset in audio_assets {
            let secs = asset.metadata.duration_ns / 1_000_000_000;
            let card = container(
                row![
                    icons::icon(icons::TAB_AUDIO, 13.0, theme::ACCENT),
                    column![
                        text(&asset.name).size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
                        text(format!("{} -> {}", format_ruler_label(secs), selected_track_name))
                            .size(10)
                            .color(theme::TEXT_SECONDARY),
                    ]
                    .spacing(1)
                    .width(Length::Fill),
                    button(icons::icon(icons::PLUS, 12.0, theme::TEXT_ON_ACCENT))
                        .padding([5, 7])
                        .style(theme::primary_button)
                        .on_press(AudioLibraryMessage::AddAudioToSelectedTrack(asset.id)),
                ]
                .spacing(6)
                .align_y(Vertical::Center),
            )
            .width(Length::Fill)
            .padding([6, 8])
            .style(theme::card);
            list_col = list_col.push(card);
        }
    }

    let presets_header =
        text(s.audio_presets_title).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY);
    let presets_grid = column![
        row![
            audio_preset_card(AudioPreset::YouTubeVoice, settings.audio_preset == Some(AudioPreset::YouTubeVoice), lang),
            audio_preset_card(AudioPreset::Music, settings.audio_preset == Some(AudioPreset::Music), lang),
        ]
        .spacing(6),
        row![
            audio_preset_card(AudioPreset::Podcast, settings.audio_preset == Some(AudioPreset::Podcast), lang),
            audio_preset_card(AudioPreset::ClearVoice, settings.audio_preset == Some(AudioPreset::ClearVoice), lang),
        ]
        .spacing(6),
        row![
            audio_preset_card(AudioPreset::Cinema, settings.audio_preset == Some(AudioPreset::Cinema), lang),
            audio_preset_card(AudioPreset::Radio, settings.audio_preset == Some(AudioPreset::Radio), lang),
        ]
        .spacing(6),
    ]
    .spacing(6);

    let fx_header =
        text(s.audio_effects_title).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY);
    let fx_row = row![
        effect_card(AudioEffectKind::Reverb, settings.has_effect(AudioEffectKind::Reverb), lang),
        effect_card(AudioEffectKind::Echo, settings.has_effect(AudioEffectKind::Echo), lang),
    ]
    .spacing(6);

    container(
        column![
            header,
            search,
            horizontal_rule(1).style(theme::divider),
            assets_header,
            scrollable(list_col).height(Length::Fill).style(theme::dark_scrollable),
            horizontal_rule(1).style(theme::divider),
            presets_header,
            presets_grid,
            horizontal_rule(1).style(theme::divider),
            fx_header,
            fx_row,
        ]
        .spacing(8)
        .height(Length::Fill),
    )
    .width(240)
    .height(Length::Fill)
    .padding(12)
    .style(theme::sidebar)
    .into()
}

fn audio_preset_card<'a>(
    preset: AudioPreset,
    active: bool,
    lang: AppLanguage,
) -> Element<'a, AudioLibraryMessage> {
    let btn = button(
        container(
            text(i18n::audio_preset_label(lang, preset))
                .size(10.5)
                .font(theme::FONT_SEMIBOLD)
                .color(if active { theme::TEXT_ON_ACCENT } else { theme::TEXT_PRIMARY })
                .wrapping(text::Wrapping::None),
        )
        .width(Length::Fill)
        .align_x(Horizontal::Center),
    )
    .width(Length::Fill)
    .padding([7, 5])
    .style(theme::filter_pill_button(active))
    .on_press(AudioLibraryMessage::ApplyAudioPreset(preset));

    icon_tooltip(btn, i18n::audio_preset_description(lang, preset), Position::Right)
}

fn effect_card<'a>(
    kind: AudioEffectKind,
    active: bool,
    lang: AppLanguage,
) -> Element<'a, AudioLibraryMessage> {
    button(
        container(
            text(i18n::audio_effect_label(lang, kind))
                .size(11)
                .font(theme::FONT_SEMIBOLD)
                .color(if active { theme::ACCENT_HOVER } else { theme::TEXT_PRIMARY }),
        )
        .width(Length::Fill)
        .align_x(Horizontal::Center),
    )
    .width(Length::Fill)
    .padding([8, 6])
    .style(theme::media_card_button(active))
    .on_press(AudioLibraryMessage::ToggleAudioEffect(kind))
    .into()
}
