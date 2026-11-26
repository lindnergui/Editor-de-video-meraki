pub mod audio;
pub mod effect_params;
pub mod effects;
pub mod export;
pub mod history;
mod history_audio;
mod history_effect_params;
mod history_effects;
pub mod preferences;
pub mod project;
pub mod timeline;
mod timeline_audio;
mod timeline_effects;
mod title_layout;

pub use audio::{
    db_to_linear, eq_band_index, AudioEffectKind, TrackAudioSettings, VoicePreset, VolumeKeyframe,
    EQ_BAND_FREQUENCIES_HZ, MAX_EQ_GAIN_DB, MAX_TRACK_GAIN_DB, MAX_VOLUME_KF_DB, MIN_EQ_GAIN_DB,
    MIN_TRACK_GAIN_DB, MIN_VOLUME_KF_DB,
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
    ApplyVoicePresetCommand, Command, HistoryError, HistoryStack, MoveClipCommand, RemoveClipCommand,
    RemoveEffectCommand, RemoveKeyframeCommand, RemoveVolumeKeyframeCommand, RenameProjectCommand,
    SetClipTransformCommand, SetClipTransitionCommand, SplitClipCommand, ToggleAudioEffectCommand,
    ToggleClipEffectCommand, ToggleEffectCommand, TrimClipCommand, UpdateClipTransformCommand,
    UpdateEffectParamsCommand, UpdateTitleCommand, UpdateTrackAudioCommand, UpsertKeyframeCommand,
    UpsertVolumeKeyframeCommand,
};
pub use preferences::{
    AppPreferences, KeymapProfile, RecentProjectEntry, ShortcutAction, MAX_RECENT_PROJECTS,
};
pub use project::{
    AssetId, EqBand, FolderId, MediaAsset, MediaFolder, MediaKind, MediaMetadata, PreviewQuality,
    Project, ProjectError,
};
pub use timeline::{Clip, ClipId, Timeline, TimelineError, Track, TrackId, TrackKind};
pub use title_layout::{title_pose, NormalizedTitleBox, TitlePose};
