pub mod audio;
pub mod audio_eq;
pub mod effect_params;
pub mod effects;
pub mod export;
pub mod history;
mod history_audio;
mod history_audio_eq;
mod history_effect_params;
mod history_effects;
mod history_timeline;
mod history_transitions;
pub mod preferences;
pub mod project;
pub mod timeline;
mod timeline_audio;
mod timeline_effects;
mod title_layout;
pub mod transitions;

pub use audio::{
    clamp_track_gain_db, db_to_linear, eq_band_index, AudioEffectKind, TrackAudioSettings,
    VoicePreset, VolumeKeyframe, EQ_BAND_FREQUENCIES_HZ, MAX_EQ_GAIN_DB, MAX_TRACK_GAIN_DB,
    MAX_VOLUME_KF_DB, MIN_EQ_GAIN_DB, MIN_TRACK_GAIN_DB, MIN_VOLUME_KF_DB,
};
pub use audio_eq::{
    auto_eq_gains_f32, clamp_eq_gain_db, clamp_eq_gains_db, compute_auto_eq, eq_band_spec,
    AudioPreset, AutoEqTarget, EqBandSpec, EqFilterType, SpectrumBands, AUTO_EQ_DAMPING_FACTOR,
    AUTO_EQ_MAX_CORRECTION_DB, AUTO_EQ_MIN_CORRECTION_DB, EQ_BAND_SPECS, EQ_BAND_WIDTHS_HZ,
};
pub use effect_params::{
    BlurParams, CinematicParams, EffectParameters, FilmParams, GlitchParams, TransitionEffectKind,
    TransitionParams, ZoomEasing, ZoomParams,
};
pub use effects::{
    ClipEffect, ClipTransform, ClipTransition, EffectKind, Keyframe, KeyframeEasing,
    KeyframeProperty, TitleOverlay, TitleStyle, TransitionEdge, TransitionKind, VideoEffect,
    VideoEffectKind,
};
pub use export::{
    ExportError, ExportFormat, ExportFrameRate, ExportJob, ExportJobId, ExportJobStatus,
    ExportPreset, ExportQuality, ExportResolution, ExportSettings,
};
pub use history::{
    AddAssetCommand, AddClipCommand, AddClipEffectCommand, AddFolderCommand, AddTitleClipCommand,
    ApplyAudioPreset, ApplyAudioPresetCommand, ApplyVoicePresetCommand, Command, HistoryError,
    HistoryStack, MoveClipCommand, RemoveClipCommand, RemoveEffectCommand, RemoveKeyframeCommand,
    RemoveTransitionCommand, RemoveVolumeKeyframeCommand, RenameProjectCommand, ResetEqBand,
    ResetEqBandCommand, SetClipTransformCommand, SetClipTransitionCommand, SetMasterVolume,
    SetMasterVolumeCommand, SetTrackEq, SetTrackEqCommand, SetTrackVolume, SetTrackVolumeCommand,
    SplitClipCommand, ToggleAudioEffectCommand, ToggleClipEffectCommand, ToggleEffectCommand,
    TrimClipCommand, UpdateClipTransformCommand, UpdateEffectParamsCommand, UpdateTitleCommand,
    UpdateTrackAudioCommand, UpdateTransitionDurationCommand, UpsertKeyframeCommand,
    UpsertTransitionCommand, UpsertVolumeKeyframeCommand,
};
pub use preferences::{
    AppLanguage, AppPreferences, KeymapProfile, ProxyMode, RecentProjectEntry, ShortcutAction,
    MAX_RECENT_PROJECTS,
};
pub use project::{
    AssetId, EqBand, FolderId, MediaAsset, MediaFolder, MediaKind, MediaMetadata, PreviewQuality,
    Project, ProjectError,
};
pub use timeline::{
    clip_source_bounds, Clip, ClipId, Timeline, TimelineError, TimelineTransition, Track, TrackId,
    TrackKind, TransitionPlacement, DEFAULT_TRANSITION_DURATION_NS,
};
pub use title_layout::{title_pose, NormalizedTitleBox, TitlePose};
