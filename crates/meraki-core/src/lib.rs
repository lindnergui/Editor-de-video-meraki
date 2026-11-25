pub mod audio;
pub mod effects;
pub mod export;
pub mod history;
mod history_audio;
mod history_effects;
pub mod project;
pub mod timeline;
mod timeline_audio;
mod timeline_effects;

pub use audio::{
    db_to_linear, eq_band_index, AudioEffectKind, TrackAudioSettings, VoicePreset, VolumeKeyframe,
    EQ_BAND_FREQUENCIES_HZ, MAX_EQ_GAIN_DB, MAX_TRACK_GAIN_DB, MAX_VOLUME_KF_DB, MIN_EQ_GAIN_DB,
    MIN_TRACK_GAIN_DB, MIN_VOLUME_KF_DB,
};
pub use effects::{
    ClipEffect, ClipTransform, ClipTransition, Keyframe, KeyframeEasing, KeyframeProperty,
    TitleOverlay, TitleStyle, TransitionEdge, TransitionKind, VideoEffectKind,
};
pub use export::{
    ExportError, ExportFormat, ExportFrameRate, ExportJob, ExportJobId, ExportJobStatus,
    ExportPreset, ExportQuality, ExportResolution, ExportSettings,
};
pub use history::{
    AddAssetCommand, AddClipCommand, AddFolderCommand, AddTitleClipCommand,
    ApplyVoicePresetCommand, Command, HistoryError, HistoryStack, MoveClipCommand, RemoveClipCommand,
    RemoveKeyframeCommand, RemoveVolumeKeyframeCommand, RenameProjectCommand,
    SetClipTransitionCommand, SplitClipCommand, ToggleAudioEffectCommand, ToggleClipEffectCommand,
    TrimClipCommand, UpdateClipTransformCommand, UpdateTitleCommand, UpdateTrackAudioCommand,
    UpsertKeyframeCommand, UpsertVolumeKeyframeCommand,
};
pub use project::{
    AssetId, EqBand, FolderId, MediaAsset, MediaFolder, MediaKind, MediaMetadata, PreviewQuality,
    Project, ProjectError,
};
pub use timeline::{Clip, ClipId, Timeline, TimelineError, Track, TrackId, TrackKind};
