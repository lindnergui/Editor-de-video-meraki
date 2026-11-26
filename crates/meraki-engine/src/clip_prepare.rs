use crate::discoverer::{self, DiscovererError};
use crate::protocol::TimelineRenderClip;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{ClipId, TimelineError, TrackKind};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error(transparent)]
    FrameSink(#[from] crate::frame_sink::FrameSinkError),
    #[error(transparent)]
    Discoverer(#[from] DiscovererError),
    #[error("falha na configuração GES ({step}): {reason}")]
    TimelineSetup { step: &'static str, reason: String },
    #[error("falha ao carregar ativo de mídia '{path}': {reason}")]
    LoadUriAsset { path: PathBuf, reason: String },
    #[error("falha ao inserir clipe '{path}' na camada GES: {reason}")]
    AddAssetClip { path: PathBuf, reason: String },
    #[error("nenhum fluxo compatível para '{0}'")]
    NoCompatibleStreams(PathBuf),
    #[error("duração inválida (zero) no clipe {0}")]
    InvalidClipDuration(ClipId),
    #[error("configuração inválida no clipe {clip_id}: {source}")]
    InvalidTransform {
        clip_id: ClipId,
        #[source]
        source: TimelineError,
    },
    #[error("parâmetro inválido '{name}': {reason}")]
    InvalidParameter { name: &'static str, reason: &'static str },
    #[error("falha ao confirmar alterações (commit) na timeline GES")]
    CommitFailed,
    #[error("falha ao alterar estado do pipeline GES para {target}: {reason}")]
    StateChange { target: String, reason: String },
    #[error("falha ao executar seek para {position_ns}ns: {reason}")]
    SeekFailed { position_ns: u64, reason: String },
    #[error("falha ao atualizar clipe {clip_id} ao vivo: {property}")]
    LiveProperty { clip_id: ClipId, property: &'static str },
}

pub(crate) enum PreparedSource {
    Media {
        asset: ges::UriClipAsset,
        track_types: ges::TrackType,
        is_image: bool,
        natural_size: Option<(i32, i32)>,
    },
    Title,
}

pub(crate) struct PreparedClip {
    pub render: TimelineRenderClip,
    pub source: PreparedSource,
}

/// Valida e prepara clipes da timeline reutilizando `UriClipAsset` em cache entre sincronizações.
pub(crate) async fn prepare_clips(
    asset_cache: &mut HashMap<String, ges::UriClipAsset>,
    mut clips: Vec<TimelineRenderClip>,
) -> Result<Vec<PreparedClip>, PipelineError> {
    clips.sort_by_key(|c| (track_kind_rank(c.track_kind), c.timeline_start_ns));
    let mut prepared = Vec::with_capacity(clips.len());

    for clip in clips {
        if clip.duration_ns == 0 {
            return Err(PipelineError::InvalidClipDuration(clip.clip_id));
        }
        clip.transform
            .validate()
            .map_err(|source| PipelineError::InvalidTransform { clip_id: clip.clip_id, source })?;
        for effect in &clip.effects {
            effect.validate().map_err(|source| PipelineError::InvalidTransform { clip_id: clip.clip_id, source })?;
        }
        if let Some(tr) = clip.transition_in {
            tr.validate(clip.duration_ns)
                .map_err(|source| PipelineError::InvalidTransform { clip_id: clip.clip_id, source })?;
        }
        if let Some(tr) = clip.transition_out {
            tr.validate(clip.duration_ns)
                .map_err(|source| PipelineError::InvalidTransform { clip_id: clip.clip_id, source })?;
        }
        for kf in &clip.keyframes {
            kf.validate(clip.duration_ns)
                .map_err(|source| PipelineError::InvalidTransform { clip_id: clip.clip_id, source })?;
        }
        for vkf in &clip.volume_keyframes {
            vkf.validate(clip.duration_ns)
                .map_err(|source| PipelineError::InvalidTransform { clip_id: clip.clip_id, source })?;
        }

        if clip.track_kind == TrackKind::Overlay {
            if let Some(title) = &clip.title {
                title
                    .validate()
                    .map_err(|source| PipelineError::InvalidTransform { clip_id: clip.clip_id, source })?;
                prepared.push(PreparedClip { render: clip, source: PreparedSource::Title });
                continue;
            }
        }

        let uri = discoverer::path_to_file_uri(&clip.path)?;
        let asset = if let Some(cached) = asset_cache.get(&uri) {
            cached.clone()
        } else {
            let loaded = ges::UriClipAsset::new_future(&uri)
                .await
                .map_err(|err| PipelineError::LoadUriAsset { path: clip.path.clone(), reason: err.to_string() })?;
            asset_cache.insert(uri, loaded.clone());
            loaded
        };

        let info = asset.info();
        let track_types = resolve_track_types(
            clip.track_kind,
            !info.video_streams().is_empty(),
            !info.audio_streams().is_empty(),
        )
        .ok_or_else(|| PipelineError::NoCompatibleStreams(clip.path.clone()))?;

        let first_video = info.video_streams().into_iter().next();
        let is_image = first_video.as_ref().is_some_and(|v| v.is_image());
        let natural_size = first_video.and_then(|v| {
            let (w, h) = (i32::try_from(v.width()).ok()?, i32::try_from(v.height()).ok()?);
            (w > 0 && h > 0).then_some((w, h))
        });

        prepared.push(PreparedClip {
            render: clip,
            source: PreparedSource::Media { asset, track_types, is_image, natural_size },
        });
    }

    Ok(prepared)
}

fn track_kind_rank(kind: TrackKind) -> u8 {
    match kind {
        TrackKind::Overlay => 0,
        TrackKind::Video => 1,
        TrackKind::Audio => 2,
    }
}

fn resolve_track_types(kind: TrackKind, has_video: bool, has_audio: bool) -> Option<ges::TrackType> {
    match kind {
        TrackKind::Audio => has_audio.then_some(ges::TrackType::AUDIO),
        TrackKind::Overlay => has_video.then_some(ges::TrackType::VIDEO),
        TrackKind::Video => {
            let mut flags = ges::TrackType::empty();
            if has_video {
                flags |= ges::TrackType::VIDEO;
            }
            if has_audio {
                flags |= ges::TrackType::AUDIO;
            }
            (!flags.is_empty()).then_some(flags)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discoverer::tests::{write_test_png_via_gst, TempFileGuard};
    use meraki_core::{ClipId, ClipTransform};

    #[test]
    fn prepare_clips_caches_uri_clip_assets_between_syncs() {
        crate::ensure_media_initialized().unwrap();
        let png = TempFileGuard::new("cache_probe", "png");
        write_test_png_via_gst(&png.0, 320, 180);

        let ctx = glib::MainContext::new();
        ctx.block_on(async move {
            let mut cache = HashMap::new();
            let make_clip = || TimelineRenderClip {
                clip_id: ClipId::new(),
                track_id: meraki_core::TrackId::new(),
                path: png.0.clone(),
                track_kind: TrackKind::Video,
                timeline_start_ns: 0,
                source_in_ns: 0,
                duration_ns: 2_000_000_000,
                transform: ClipTransform::default(),
                title: None,
                effects: Vec::new(),
                transition_in: None,
                transition_out: None,
                keyframes: Vec::new(),
                volume_keyframes: Vec::new(),
            };

            let first = prepare_clips(&mut cache, vec![make_clip()]).await.unwrap();
            assert_eq!(cache.len(), 1);
            let second = prepare_clips(&mut cache, vec![make_clip()]).await.unwrap();
            assert_eq!(cache.len(), 1);

            match (&first[0].source, &second[0].source) {
                (
                    PreparedSource::Media { asset: a, .. },
                    PreparedSource::Media { asset: b, .. },
                ) => assert_eq!(a, b),
                _ => panic!("esperado PreparedSource::Media"),
            }
        });
    }
}
