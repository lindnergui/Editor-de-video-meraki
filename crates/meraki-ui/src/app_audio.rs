use crate::app::{MerakiApp, Message};
use crate::components::audio_library::AudioLibraryMessage;
use crate::components::audio_timeline::AudioTimelineMessage;
use crate::components::eq_canvas::EqMessage;
use crate::components::mixer::MixerMessage;
use crate::components::player;
use crate::playback::{self, PlaybackMessage};
use crate::views::audio::{self, AudioMessage};
use iced::Task;
use meraki_core::{
    eq_band_index, AddClipCommand, ApplyVoicePresetCommand, AudioEffectKind, EqBand,
    RemoveVolumeKeyframeCommand, ToggleAudioEffectCommand, TrackAudioSettings, TrackId,
    TrackKind, UpdateTrackAudioCommand, UpsertVolumeKeyframeCommand, VoicePreset, VolumeKeyframe,
    MAX_EQ_GAIN_DB, MAX_TRACK_GAIN_DB, MIN_EQ_GAIN_DB, MIN_TRACK_GAIN_DB,
};
use meraki_engine::EngineCommand;

impl MerakiApp {
    /// Garante as 3 trilhas reais do Mixer (`A1`, `A2 Música`, `A3 SFX`) e seleção válida.
    pub fn ensure_audio_tab_ready(&mut self) {
        let audio_ids = self.project.timeline.ensure_mixer_audio_tracks();
        let valid = self
            .audio
            .selected_track_id
            .and_then(|id| self.project.timeline.track(id))
            .is_some_and(|t| t.kind == TrackKind::Audio);
        if !valid {
            self.audio.selected_track_id = audio_ids.first().copied();
        }
    }

    pub fn active_audio_track_id(&self) -> Option<TrackId> {
        audio::resolve_selected_audio_track(&self.project, self.audio.selected_track_id).map(|t| t.id)
    }

    pub fn apply_voice_preset(&mut self, track_id: TrackId, preset: VoicePreset) -> Result<(), String> {
        self.execute_audio_command(Box::new(ApplyVoicePresetCommand::new(track_id, preset)))?;
        if let Some(track) = self.project.timeline.track(track_id) {
            let s = track.audio.clone();
            self.send_live_eq_and_voice(track_id, &s);
        }
        Ok(())
    }

    pub fn toggle_audio_effect(&mut self, track_id: TrackId, effect: AudioEffectKind) -> Result<(), String> {
        self.execute_audio_command(Box::new(ToggleAudioEffectCommand::new(track_id, effect)))?;
        if let Some(track) = self.project.timeline.track(track_id) {
            let enabled = track.audio.has_effect(effect);
            self.send_engine_command(EngineCommand::ToggleTrackAudioEffect { track_id, effect, enabled });
        }
        Ok(())
    }

    pub(crate) fn handle_audio_message(&mut self, message: AudioMessage) -> Task<Message> {
        self.ensure_audio_tab_ready();
        match message {
            AudioMessage::Library(lib_msg) => match lib_msg {
                AudioLibraryMessage::SearchChanged(q) => self.audio.search_query = q,
                AudioLibraryMessage::ImportAudioClicked => {
                    return Task::perform(audio::pick_audio_files(), |paths| {
                        Message::Audio(AudioMessage::AudioFilesPicked(paths))
                    });
                }
                AudioLibraryMessage::AddAudioToSelectedTrack(asset_id) => {
                    if let Some(track_id) = self.active_audio_track_id() {
                        if let Some(clip) = playback::build_clip_for_asset_on_track(&self.project, asset_id, track_id) {
                            let clip_id = clip.id;
                            if self.execute_timeline_command(Box::new(AddClipCommand::new(track_id, clip))).is_ok() {
                                self.playback.selected_clip_id = Some(clip_id);
                            }
                        }
                    }
                }
                AudioLibraryMessage::ApplyVoicePreset(preset) => {
                    if let Some(tid) = self.active_audio_track_id() {
                        let _ = self.apply_voice_preset(tid, preset);
                    }
                }
                AudioLibraryMessage::ToggleAudioEffect(effect) => {
                    if let Some(tid) = self.active_audio_track_id() {
                        let _ = self.toggle_audio_effect(tid, effect);
                    }
                }
            },
            AudioMessage::AudioFilesPicked(paths) => {
                let sfx_folder = self
                    .project
                    .folders
                    .iter()
                    .find(|f| f.name == "Trilhas & SFX")
                    .map(|f| f.id);
                for path in paths {
                    let _ = self.import_media(path, sfx_folder, false);
                }
            }
            AudioMessage::Player(p_msg) => {
                let pb = player::into_playback_message(p_msg, &self.project, self.playback.playhead_ns);
                self.handle_playback_message(pb);
            }
            AudioMessage::Eq(eq_msg) => self.handle_eq_message(eq_msg),
            AudioMessage::Mixer(mix_msg) => self.handle_mixer_message(mix_msg),
            AudioMessage::Timeline(tl_msg) => self.handle_audio_timeline_message(tl_msg),
        }
        Task::none()
    }

    fn handle_eq_message(&mut self, msg: EqMessage) {
        let Some(track_id) = self.active_audio_track_id() else {
            return;
        };
        match msg {
            EqMessage::BandDragged { band, gain_db } => {
                self.preview_eq_band(track_id, band, gain_db);
                self.send_engine_command(EngineCommand::UpdateEqBand { track_id, band, gain_db });
            }
            EqMessage::BandReleased { band, gain_db } => {
                let clamped = gain_db.clamp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB);
                self.commit_eq_drag(track_id, band, clamped);
                self.send_engine_command(EngineCommand::UpdateEqBand { track_id, band, gain_db: clamped });
            }
            EqMessage::ResetEq => {
                if let Some(track) = self.project.timeline.track(track_id) {
                    let mut next = track.audio.clone();
                    next.eq_gains_db = [0.0; 4];
                    next.voice_preset = None;
                    if self.execute_audio_command(Box::new(UpdateTrackAudioCommand::new(track_id, next))).is_ok() {
                        for band in [EqBand::Low, EqBand::LowMid, EqBand::Mid, EqBand::High] {
                            self.send_engine_command(EngineCommand::UpdateEqBand { track_id, band, gain_db: 0.0 });
                        }
                    }
                }
            }
            EqMessage::NoiseReductionToggled(enabled) => {
                if let Some(track) = self.project.timeline.track(track_id) {
                    let mut next = track.audio.clone();
                    next.noise_reduction_enabled = enabled;
                    let strength = next.noise_reduction_strength;
                    if self.execute_audio_command(Box::new(UpdateTrackAudioCommand::new(track_id, next))).is_ok() {
                        self.send_engine_command(EngineCommand::SetNoiseReduction { track_id, enabled, strength });
                    }
                }
            }
            EqMessage::NoiseReductionStrengthChanged(strength) => {
                let clamped = strength.clamp(0.0, 1.0);
                if let Some(track) = self.project.timeline.track_mut(track_id) {
                    if self.audio.nr_drag_origin.is_none() {
                        self.audio.nr_drag_origin = Some((track_id, track.audio.clone()));
                    }
                    track.audio.noise_reduction_strength = clamped;
                    let enabled = track.audio.noise_reduction_enabled;
                    self.send_engine_command(EngineCommand::SetNoiseReduction { track_id, enabled, strength: clamped });
                }
            }
            EqMessage::NoiseReductionStrengthReleased => {
                if let Some((orig_tid, orig)) = self.audio.nr_drag_origin.take() {
                    if let Some(final_s) = self.project.timeline.track(orig_tid).map(|t| t.audio.clone()) {
                        let _ = self.project.timeline.update_track_audio(orig_tid, orig);
                        let _ = self.execute_audio_command(Box::new(UpdateTrackAudioCommand::new(orig_tid, final_s)));
                    }
                }
            }
            EqMessage::VoiceLevelerToggled(enabled) => {
                if let Some(track) = self.project.timeline.track(track_id) {
                    let mut next = track.audio.clone();
                    next.voice_leveler_enabled = enabled;
                    if self.execute_audio_command(Box::new(UpdateTrackAudioCommand::new(track_id, next))).is_ok() {
                        self.send_engine_command(EngineCommand::SetVoiceLeveler { track_id, enabled });
                    }
                }
            }
        }
    }

    fn handle_mixer_message(&mut self, msg: MixerMessage) {
        match msg {
            MixerMessage::SelectTrack(track_id) => self.audio.selected_track_id = Some(track_id),
            MixerMessage::ToggleMute(track_id) => {
                if let Some(track) = self.project.timeline.track(track_id) {
                    let mut next = track.audio.clone();
                    next.muted = !next.muted;
                    let muted = next.muted;
                    if self.execute_audio_command(Box::new(UpdateTrackAudioCommand::new(track_id, next))).is_ok() {
                        self.send_engine_command(EngineCommand::SetTrackMute { track_id, muted });
                    }
                }
            }
            MixerMessage::ToggleSolo(track_id) => {
                if let Some(track) = self.project.timeline.track(track_id) {
                    let mut next = track.audio.clone();
                    next.solo = !next.solo;
                    let solo = next.solo;
                    if self.execute_audio_command(Box::new(UpdateTrackAudioCommand::new(track_id, next))).is_ok() {
                        self.send_engine_command(EngineCommand::SetTrackSolo { track_id, solo });
                    }
                }
            }
            MixerMessage::TrackGainChanged { track_id, gain_db } => {
                let clamped = gain_db.clamp(MIN_TRACK_GAIN_DB, MAX_TRACK_GAIN_DB);
                if let Some(track) = self.project.timeline.track_mut(track_id) {
                    if self.audio.gain_drag_origin.is_none() {
                        self.audio.gain_drag_origin = Some((track_id, track.audio.clone()));
                    }
                    track.audio.gain_db = clamped;
                    track.gain_db = clamped;
                    self.send_engine_command(EngineCommand::SetTrackGain { track_id, gain_db: clamped });
                }
            }
            MixerMessage::TrackGainReleased(track_id) => {
                if let Some((orig_tid, orig)) = self.audio.gain_drag_origin.take() {
                    if let Some(final_s) = self.project.timeline.track(orig_tid).map(|t| t.audio.clone()) {
                        let _ = self.project.timeline.update_track_audio(orig_tid, orig);
                        let _ = self.execute_audio_command(Box::new(UpdateTrackAudioCommand::new(orig_tid, final_s)));
                    }
                } else if let Some(track) = self.project.timeline.track(track_id) {
                    let s = track.audio.clone();
                    self.send_engine_command(EngineCommand::SetTrackGain { track_id, gain_db: s.gain_db });
                }
            }
            MixerMessage::MasterVolumeChanged(vol) => {
                self.handle_playback_message(PlaybackMessage::SetMasterVolume(vol));
            }
        }
    }

    fn handle_audio_timeline_message(&mut self, msg: AudioTimelineMessage) {
        match msg {
            AudioTimelineMessage::ZoomIn => self.audio.timeline.zoom_in(),
            AudioTimelineMessage::ZoomOut => self.audio.timeline.zoom_out(),
            AudioTimelineMessage::Scrub(pos_ns) => self.handle_playback_message(PlaybackMessage::Seek(pos_ns)),
            AudioTimelineMessage::SelectTrack(tid) => self.audio.selected_track_id = Some(tid),
            AudioTimelineMessage::SelectClip(cid) => self.playback.selected_clip_id = cid,
            AudioTimelineMessage::UpsertVolumeKeyframe { clip_id, previous_offset_ns, keyframe } => {
                self.playback.selected_clip_id = Some(clip_id);
                self.upsert_volume_keyframe_with_move(clip_id, previous_offset_ns, keyframe);
            }
            AudioTimelineMessage::RemoveVolumeKeyframe { clip_id, time_offset_ns } => {
                let _ = self.execute_timeline_command(Box::new(RemoveVolumeKeyframeCommand::new(clip_id, time_offset_ns)));
            }
        }
    }

    fn upsert_volume_keyframe_with_move(
        &mut self,
        clip_id: meraki_core::ClipId,
        previous_offset_ns: Option<u64>,
        keyframe: VolumeKeyframe,
    ) {
        if let Some(old_t) = previous_offset_ns.filter(|&t| t != keyframe.time_offset_ns) {
            let _ = self.execute_timeline_command(Box::new(RemoveVolumeKeyframeCommand::new(clip_id, old_t)));
        }
        let _ = self.execute_timeline_command(Box::new(UpsertVolumeKeyframeCommand::new(clip_id, keyframe)));
    }

    fn preview_eq_band(&mut self, track_id: TrackId, band: EqBand, gain_db: f32) {
        if let Some(track) = self.project.timeline.track_mut(track_id) {
            if self.audio.eq_drag_origin.is_none() {
                self.audio.eq_drag_origin = Some((track_id, track.audio.clone()));
            }
            track.audio.eq_gains_db[eq_band_index(band)] = gain_db.clamp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB);
            track.audio.voice_preset = None;
        }
    }

    fn commit_eq_drag(&mut self, track_id: TrackId, band: EqBand, gain_db: f32) {
        if let Some((orig_tid, orig)) = self.audio.eq_drag_origin.take() {
            if let Some(mut final_s) = self.project.timeline.track(orig_tid).map(|t| t.audio.clone()) {
                final_s.eq_gains_db[eq_band_index(band)] = gain_db;
                final_s.voice_preset = None;
                let _ = self.project.timeline.update_track_audio(orig_tid, orig);
                let _ = self.execute_audio_command(Box::new(UpdateTrackAudioCommand::new(orig_tid, final_s)));
            }
        } else if let Some(track) = self.project.timeline.track(track_id) {
            let mut next = track.audio.clone();
            next.eq_gains_db[eq_band_index(band)] = gain_db;
            next.voice_preset = None;
            let _ = self.execute_audio_command(Box::new(UpdateTrackAudioCommand::new(track_id, next)));
        }
    }

    fn execute_audio_command(&mut self, cmd: Box<dyn meraki_core::Command>) -> Result<(), String> {
        if let Err(err) = self.history.execute(cmd, &mut self.project) {
            let msg = err.to_string();
            self.last_error = Some(msg.clone());
            return Err(msg);
        }
        self.mark_project_dirty();
        Ok(())
    }

    fn send_live_eq_and_voice(&mut self, track_id: TrackId, settings: &TrackAudioSettings) {
        for band in [EqBand::Low, EqBand::LowMid, EqBand::Mid, EqBand::High] {
            self.send_engine_command(EngineCommand::UpdateEqBand {
                track_id,
                band,
                gain_db: settings.eq_gains_db[eq_band_index(band)],
            });
        }
        self.send_engine_command(EngineCommand::SetNoiseReduction {
            track_id,
            enabled: settings.noise_reduction_enabled,
            strength: settings.noise_reduction_strength,
        });
        self.send_engine_command(EngineCommand::SetVoiceLeveler {
            track_id,
            enabled: settings.voice_leveler_enabled,
        });
    }

    fn send_engine_command(&mut self, cmd: EngineCommand) {
        if let Err(err) = self.engine.send(cmd) {
            self.last_error = Some(err.to_string());
        }
    }
}
