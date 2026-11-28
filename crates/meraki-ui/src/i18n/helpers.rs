use super::strings;
use crate::app_delivery::{format_eta, CoverSlot};
use crate::navigation::WorkspaceTab;
use crate::views::files::MediaFilter;
use meraki_core::{
    AppLanguage, AudioEffectKind, AudioPreset, ClipEffect, EffectParameters, EqBand, ExportFormat,
    ExportPreset, ExportQuality, ExportResolution, KeymapProfile, MediaAsset, MediaKind, TitleStyle,
    TransitionEffectKind, TransitionKind, VideoEffectKind, VoicePreset,
};

pub fn tab_label(lang: AppLanguage, tab: WorkspaceTab) -> &'static str {
    let s = strings(lang);
    match tab {
        WorkspaceTab::Files => s.tab_files,
        WorkspaceTab::Edit => s.tab_edit,
        WorkspaceTab::Effects => s.tab_effects,
        WorkspaceTab::Audio => s.tab_audio,
        WorkspaceTab::Delivery => s.tab_delivery,
    }
}

pub fn keymap_subtitle(lang: AppLanguage, profile: KeymapProfile) -> &'static str {
    let s = strings(lang);
    match profile {
        KeymapProfile::PremierePro => s.keymap_sub_premiere,
        KeymapProfile::DaVinciResolve => s.keymap_sub_davinci,
        KeymapProfile::CapCut => s.keymap_sub_capcut,
        KeymapProfile::VegasPro => s.keymap_sub_vegas,
    }
}

pub fn format_delete_project(lang: AppLanguage, name: &str) -> String {
    match lang {
        AppLanguage::PtBr => format!("Excluir \"{name}\"?"),
        AppLanguage::EnUs => format!("Delete \"{name}\"?"),
    }
}

pub fn format_delete_project_desc(lang: AppLanguage, name: &str) -> String {
    match lang {
        AppLanguage::PtBr => format!(
            "O projeto \"{name}\" será removido dos recentes e apagado definitivamente. Esta ação não pode ser desfeita."
        ),
        AppLanguage::EnUs => format!(
            "Project \"{name}\" will be removed from recents and permanently deleted. This action cannot be undone."
        ),
    }
}

pub fn format_shortcuts_badge(lang: AppLanguage, profile_name: &str) -> String {
    match lang {
        AppLanguage::PtBr => format!("Atalhos ({profile_name})"),
        AppLanguage::EnUs => format!("Shortcuts ({profile_name})"),
    }
}

pub fn format_unsaved_desc(lang: AppLanguage, project_name: &str) -> String {
    match lang {
        AppLanguage::PtBr => {
            format!("O projeto \"{project_name}\" possui alterações que ainda não foram salvas.")
        }
        AppLanguage::EnUs => format!("Project \"{project_name}\" has unsaved changes."),
    }
}

pub fn format_proxy_cache_cleared(lang: AppLanguage, freed_bytes: u64) -> String {
    let mb = (freed_bytes as f64) / (1024.0 * 1024.0);
    match lang {
        AppLanguage::PtBr => format!("Cache de proxies limpo ({mb:.1} MB liberados)"),
        AppLanguage::EnUs => format!("Proxy cache cleared ({mb:.1} MB freed)"),
    }
}

pub fn format_file_count(lang: AppLanguage, count: usize) -> String {
    match lang {
        AppLanguage::PtBr => format!("{count} arquivo(s)"),
        AppLanguage::EnUs => {
            if count == 1 { "1 file".to_string() } else { format!("{count} files") }
        }
    }
}

pub fn format_clip_fx_summary(lang: AppLanguage, fx_count: usize, kf_count: usize) -> String {
    match lang {
        AppLanguage::PtBr => format!("{fx_count} efeito(s) • {kf_count} keyframe(s)"),
        AppLanguage::EnUs => format!("{fx_count} effect(s) • {kf_count} keyframe(s)"),
    }
}

pub fn format_keyframe_instant(lang: AppLanguage, sec: f64, kf_count: usize) -> String {
    match lang {
        AppLanguage::PtBr => format!("Instante: {sec:.2}s • {kf_count} keyframe(s)"),
        AppLanguage::EnUs => format!("Time: {sec:.2}s • {kf_count} keyframe(s)"),
    }
}

pub fn format_queue_counts(lang: AppLanguage, queued: usize, history: usize) -> String {
    match lang {
        AppLanguage::PtBr => format!("{queued} na fila • {history} no histórico"),
        AppLanguage::EnUs => format!("{queued} queued • {history} in history"),
    }
}

pub fn format_eta_localized(lang: AppLanguage, eta_seconds: Option<u64>) -> String {
    match (lang, eta_seconds) {
        (AppLanguage::EnUs, None) => "Calculating...".to_string(),
        _ => format_eta(eta_seconds),
    }
}

pub fn media_filter_label(lang: AppLanguage, filter: MediaFilter) -> &'static str {
    let s = strings(lang);
    match filter {
        MediaFilter::All => s.files_filter_all,
        MediaFilter::Videos => s.files_filter_videos,
        MediaFilter::Audios => s.files_filter_audios,
        MediaFilter::Images => s.files_filter_images,
    }
}

pub fn media_kind_label(lang: AppLanguage, kind: MediaKind) -> &'static str {
    match (lang, kind) {
        (AppLanguage::PtBr, MediaKind::Video) => "Vídeo",
        (AppLanguage::EnUs, MediaKind::Video) => "Video",
        (AppLanguage::PtBr, MediaKind::Audio) => "Áudio",
        (AppLanguage::EnUs, MediaKind::Audio) => "Audio",
        (AppLanguage::PtBr, MediaKind::Image) => "Imagem",
        (AppLanguage::EnUs, MediaKind::Image) => "Image",
    }
}

pub fn format_asset_resolution(lang: AppLanguage, asset: &MediaAsset) -> String {
    match (asset.metadata.width, asset.metadata.height) {
        (Some(w), Some(h)) => format!("{w}x{h}"),
        _ if asset.kind == MediaKind::Audio => strings(lang).files_audio_only.to_string(),
        _ => "—".to_string(),
    }
}

pub fn voice_preset_label(lang: AppLanguage, preset: VoicePreset) -> &'static str {
    match (lang, preset) {
        (AppLanguage::PtBr, _) => preset.label(),
        (AppLanguage::EnUs, VoicePreset::Podcast) => "Podcast Voice",
        (AppLanguage::EnUs, VoicePreset::ClearVoice) => "Clear Voice",
        (AppLanguage::EnUs, VoicePreset::EnhanceVoice) => "Enhance Voice",
        (AppLanguage::EnUs, VoicePreset::Radio) => "Radio Voice",
    }
}

pub fn audio_effect_label(lang: AppLanguage, kind: AudioEffectKind) -> &'static str {
    match (lang, kind) {
        (AppLanguage::PtBr, _) => kind.label(),
        (AppLanguage::EnUs, AudioEffectKind::Reverb) => "Reverb",
        (AppLanguage::EnUs, AudioEffectKind::Echo) => "Echo",
    }
}

pub fn eq_band_label(lang: AppLanguage, band: EqBand) -> &'static str {
    match (lang, band) {
        (AppLanguage::PtBr, EqBand::Low) => "Graves",
        (AppLanguage::PtBr, EqBand::LowMid) => "Médios-Graves",
        (AppLanguage::PtBr, EqBand::Mid) => "Médios",
        (AppLanguage::PtBr, EqBand::High) => "Agudos",
        (AppLanguage::EnUs, EqBand::Low) => "Low",
        (AppLanguage::EnUs, EqBand::LowMid) => "Low-Mid",
        (AppLanguage::EnUs, EqBand::Mid) => "Mid",
        (AppLanguage::EnUs, EqBand::High) => "High",
    }
}

pub fn eq_band_freq_label(band: EqBand) -> &'static str {
    match band {
        EqBand::Low => "90 Hz",
        EqBand::LowMid => "320 Hz",
        EqBand::Mid => "2.2 kHz",
        EqBand::High => "8.5 kHz",
    }
}

pub fn audio_preset_label(lang: AppLanguage, preset: AudioPreset) -> &'static str {
    match (lang, preset) {
        (AppLanguage::PtBr, _) => preset.label(),
        (AppLanguage::EnUs, AudioPreset::YouTubeVoice) => "Speech / YouTube",
        (AppLanguage::EnUs, AudioPreset::Music) => "Music",
        (AppLanguage::EnUs, AudioPreset::Podcast) => "Podcast",
        (AppLanguage::EnUs, AudioPreset::ClearVoice) => "Clear Voice",
        (AppLanguage::EnUs, AudioPreset::Cinema) => "Cinema",
        (AppLanguage::EnUs, AudioPreset::Radio) => "Radio Voice",
        (AppLanguage::EnUs, AudioPreset::Flat) => "Flat",
    }
}

pub fn audio_preset_description(lang: AppLanguage, preset: AudioPreset) -> &'static str {
    match (lang, preset) {
        (AppLanguage::PtBr, _) => preset.description(),
        (AppLanguage::EnUs, AudioPreset::YouTubeVoice) => {
            "Cleans muddy lows, brings out vocal clarity, and enables the voice leveler."
        }
        (AppLanguage::EnUs, AudioPreset::Music) => {
            "Tight low end with smooth high-end sheen for music tracks."
        }
        (AppLanguage::EnUs, AudioPreset::Podcast) => {
            "Warm vocal presence with automatic voice leveler enabled."
        }
        (AppLanguage::EnUs, AudioPreset::ClearVoice) => {
            "Maximum focus on speech intelligibility for narration and tutorials."
        }
        (AppLanguage::EnUs, AudioPreset::Cinema) => {
            "Full-bodied lows and crisp cinematic highs."
        }
        (AppLanguage::EnUs, AudioPreset::Radio) => {
            "Classic broadcast mid-focused vocal character."
        }
        (AppLanguage::EnUs, AudioPreset::Flat) => {
            "Neutral 0 dB curve with no equalization changes."
        }
    }
}

pub fn transition_effect_info(lang: AppLanguage, kind: TransitionEffectKind) -> (&'static str, &'static str) {
    match (lang, kind) {
        (AppLanguage::PtBr, TransitionEffectKind::Fade) => ("Fade", "Dissolver entrada e saída"),
        (AppLanguage::PtBr, TransitionEffectKind::ZoomTransition) => ("Zoom", "Aproximação dinâmica"),
        (AppLanguage::PtBr, TransitionEffectKind::BlurTransition) => ("Desfoque", "Transição com desfoque"),
        (AppLanguage::EnUs, TransitionEffectKind::Fade) => ("Fade", "Fade in and out dissolve"),
        (AppLanguage::EnUs, TransitionEffectKind::ZoomTransition) => ("Zoom", "Dynamic zoom transition"),
        (AppLanguage::EnUs, TransitionEffectKind::BlurTransition) => ("Blur", "Soft blur transition"),
    }
}

pub fn title_style_info(lang: AppLanguage, style: TitleStyle) -> (&'static str, &'static str) {
    match (lang, style) {
        (AppLanguage::PtBr, TitleStyle::ModernText) => ("Texto Moderno", "Título limpo centralizado"),
        (AppLanguage::PtBr, TitleStyle::QuickCaption) => ("Legenda Rápida", "Rodapé de fácil leitura"),
        (AppLanguage::PtBr, TitleStyle::Title3D) => ("Título 3D", "Destaque em negrito 3D"),
        (AppLanguage::EnUs, TitleStyle::ModernText) => ("Modern Text", "Clean centered title"),
        (AppLanguage::EnUs, TitleStyle::QuickCaption) => ("Quick Caption", "Readable lower-third caption"),
        (AppLanguage::EnUs, TitleStyle::Title3D) => ("3D Title", "Bold 3D headline"),
    }
}

pub fn video_effect_info(lang: AppLanguage, kind: VideoEffectKind) -> (&'static str, &'static str) {
    match (lang, kind) {
        (AppLanguage::PtBr, VideoEffectKind::Zoom) => ("Zoom", "Aproximação ou afastamento suave"),
        (AppLanguage::PtBr, VideoEffectKind::FilmStyle) => ("Estilo Filme", "Paleta retrô de película"),
        (AppLanguage::PtBr, VideoEffectKind::Cinematic) => ("Cinemático", "Contraste dramático de cinema"),
        (AppLanguage::PtBr, VideoEffectKind::Glitch) => ("Glitch", "Distúrbio digital estilizado"),
        (AppLanguage::PtBr, VideoEffectKind::Blur) => ("Desfoque", "Desfoque gaussiano ajustável"),
        (AppLanguage::PtBr, VideoEffectKind::Transition) => ("Transição", "Transição suave"),
        (AppLanguage::EnUs, VideoEffectKind::Zoom) => ("Zoom", "Smooth zoom in or out"),
        (AppLanguage::EnUs, VideoEffectKind::FilmStyle) => ("Film Look", "Retro film palette"),
        (AppLanguage::EnUs, VideoEffectKind::Cinematic) => ("Cinematic", "Dramatic cinema contrast"),
        (AppLanguage::EnUs, VideoEffectKind::Glitch) => ("Glitch", "Stylized digital glitch"),
        (AppLanguage::EnUs, VideoEffectKind::Blur) => ("Blur", "Adjustable Gaussian blur"),
        (AppLanguage::EnUs, VideoEffectKind::Transition) => ("Transition", "Smooth transition"),
    }
}

pub fn clip_effect_label(lang: AppLanguage, effect: &ClipEffect) -> &str {
    if lang == AppLanguage::PtBr {
        return effect.name.as_str();
    }
    match effect.kind {
        VideoEffectKind::Transition => match effect.params {
            EffectParameters::Transition(p) => transition_effect_info(lang, p.kind).0,
            _ => "Transition",
        },
        other => video_effect_info(lang, other).0,
    }
}

pub fn cover_slot_label(lang: AppLanguage, slot: CoverSlot) -> &'static str {
    match (lang, slot) {
        (AppLanguage::PtBr, _) => slot.label(),
        (AppLanguage::EnUs, CoverSlot::Start) => "Start",
        (AppLanguage::EnUs, CoverSlot::Middle) => "Middle",
        (AppLanguage::EnUs, CoverSlot::End) => "End",
    }
}

pub fn export_preset_label(lang: AppLanguage, preset: ExportPreset) -> &'static str {
    match (lang, preset) {
        (AppLanguage::PtBr, _) => preset.label(),
        (AppLanguage::EnUs, ExportPreset::YouTube) => "YouTube",
        (AppLanguage::EnUs, ExportPreset::TikTokReels) => "TikTok & Reels",
        (AppLanguage::EnUs, ExportPreset::Instagram) => "Instagram",
        (AppLanguage::EnUs, ExportPreset::WhatsAppLite) => "WhatsApp Lite",
        (AppLanguage::EnUs, ExportPreset::MasterProRes) => "High Quality ProRes",
        (AppLanguage::EnUs, ExportPreset::AudioOnly) => "Audio Only",
    }
}

pub fn export_preset_card(lang: AppLanguage, preset: ExportPreset) -> (&'static str, &'static str, &'static str) {
    match (lang, preset) {
        (AppLanguage::PtBr, ExportPreset::YouTube) => ("YouTube", "1080p ou 4K, otimizado para web", "16:9"),
        (AppLanguage::PtBr, ExportPreset::TikTokReels) => ("TikTok & Reels", "9:16 vertical, alta taxa de bits", "9:16"),
        (AppLanguage::PtBr, ExportPreset::Instagram) => ("Instagram", "1:1 ou 4:5", "1:1"),
        (AppLanguage::PtBr, ExportPreset::WhatsAppLite) => ("WhatsApp (Leve)", "Arquivo leve, fácil compartilhamento", "720p"),
        (AppLanguage::PtBr, ExportPreset::MasterProRes) => ("Alta Qualidade (ProRes)", "Mestre / MOV / ProRes", "MOV"),
        (AppLanguage::PtBr, ExportPreset::AudioOnly) => ("Apenas Áudio (MP3/WAV)", "Somente trilha de áudio", "ÁUDIO"),
        (AppLanguage::EnUs, ExportPreset::YouTube) => ("YouTube", "1080p or 4K, web-optimized", "16:9"),
        (AppLanguage::EnUs, ExportPreset::TikTokReels) => ("TikTok & Reels", "9:16 vertical, high bitrate", "9:16"),
        (AppLanguage::EnUs, ExportPreset::Instagram) => ("Instagram", "1:1 or 4:5 feed ready", "1:1"),
        (AppLanguage::EnUs, ExportPreset::WhatsAppLite) => ("WhatsApp (Lite)", "Compact file, easy sharing", "720p"),
        (AppLanguage::EnUs, ExportPreset::MasterProRes) => ("High Quality (ProRes)", "Master / MOV / ProRes", "MOV"),
        (AppLanguage::EnUs, ExportPreset::AudioOnly) => ("Audio Only (MP3/WAV)", "Audio track only", "AUDIO"),
    }
}

pub fn export_format_label(lang: AppLanguage, fmt: ExportFormat) -> &'static str {
    match (lang, fmt) {
        (AppLanguage::PtBr, _) => fmt.label(),
        (AppLanguage::EnUs, ExportFormat::Mp4H264) => "MP4 (H.264 + AAC)",
        (AppLanguage::EnUs, ExportFormat::MovProRes) => "MOV (Apple ProRes)",
        (AppLanguage::EnUs, ExportFormat::AudioWav) => "WAV (PCM Audio)",
        (AppLanguage::EnUs, ExportFormat::AudioMp3) => "MP3 (MPEG Audio)",
    }
}

pub fn export_resolution_label(lang: AppLanguage, res: ExportResolution) -> &'static str {
    match (lang, res) {
        (AppLanguage::PtBr, _) => res.label(),
        (AppLanguage::EnUs, ExportResolution::Sd720p) => "720p (1280x720)",
        (AppLanguage::EnUs, ExportResolution::FullHd1080p) => "1080p (1920x1080)",
        (AppLanguage::EnUs, ExportResolution::Uhd4k) => "4K UHD (3840x2160)",
        (AppLanguage::EnUs, ExportResolution::Vertical1080x1920) => "Vertical 9:16 (1080x1920)",
        (AppLanguage::EnUs, ExportResolution::Square1080x1080) => "Square 1:1 (1080x1080)",
    }
}

pub fn export_quality_label(lang: AppLanguage, q: ExportQuality) -> &'static str {
    match (lang, q) {
        (AppLanguage::PtBr, _) => q.label(),
        (AppLanguage::EnUs, ExportQuality::Low) => "Low",
        (AppLanguage::EnUs, ExportQuality::Medium) => "Medium",
        (AppLanguage::EnUs, ExportQuality::High) => "High",
        (AppLanguage::EnUs, ExportQuality::Master) => "Master",
    }
}

pub fn transition_kind_label(lang: AppLanguage, kind: TransitionKind) -> &'static str {
    let s = strings(lang);
    match kind {
        TransitionKind::Crossfade => s.transition_kind_crossfade,
        TransitionKind::Blur => s.transition_kind_blur,
        TransitionKind::Zoom => s.transition_kind_zoom,
    }
}

pub fn transition_kind_short(lang: AppLanguage, kind: TransitionKind) -> &'static str {
    match (lang, kind) {
        (_, TransitionKind::Crossfade) => "Fade",
        (AppLanguage::PtBr, TransitionKind::Blur) => "Desf.",
        (AppLanguage::EnUs, TransitionKind::Blur) => "Blur",
        (_, TransitionKind::Zoom) => "Zoom",
    }
}
