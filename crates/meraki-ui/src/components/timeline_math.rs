use iced::Point;
use meraki_core::{Clip, ClipId, Timeline, TrackId, TrackKind};

pub const RULER_HEIGHT: f32 = 28.0;
pub const LEFT_PADDING: f32 = 12.0;
pub const TRACK_HEIGHT: f32 = 46.0;
pub const TRACK_GAP: f32 = 6.0;
pub const TRACK_STRIDE: f32 = TRACK_HEIGHT + TRACK_GAP;
pub const CLIP_RADIUS: f32 = 6.0;
pub const TRIM_HANDLE_WIDTH: f32 = 8.0;
pub const MIN_CLIP_DURATION_NS: u64 = 100_000_000;
pub const DEFAULT_PX_PER_SEC: f32 = 25.0;
pub const MIN_PX_PER_SEC: f32 = 10.0;
pub const MAX_PX_PER_SEC: f32 = 300.0;
pub const SNAP_THRESHOLD_PX: f32 = 8.0;

/// Ferramenta ativa na barra lateral esquerda da timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimelineTool {
    #[default]
    Select,
    Razor,
}

/// Estado visual compartilhado da régua, zoom e ferramentas da timeline.
#[derive(Debug, Clone)]
pub struct TimelineViewState {
    pub tool: TimelineTool,
    pub snap_enabled: bool,
    pub px_per_sec: f32,
    pub scroll_x_px: f32,
}

impl Default for TimelineViewState {
    fn default() -> Self {
        Self {
            tool: TimelineTool::Select,
            snap_enabled: true,
            px_per_sec: DEFAULT_PX_PER_SEC,
            scroll_x_px: 0.0,
        }
    }
}

impl TimelineViewState {
    pub fn zoom_in(&mut self) {
        self.px_per_sec = (self.px_per_sec * 1.25).clamp(MIN_PX_PER_SEC, MAX_PX_PER_SEC);
    }

    pub fn zoom_out(&mut self) {
        self.px_per_sec = (self.px_per_sec / 1.25).clamp(MIN_PX_PER_SEC, MAX_PX_PER_SEC);
    }

    /// Aplica fator de zoom mantendo a coordenada X da agulha (`anchor_ns`) ancorada na tela.
    pub fn zoom_anchored_at(&mut self, factor: f32, anchor_ns: u64) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let old_px = self.px_per_sec;
        let new_px = (old_px * factor).clamp(MIN_PX_PER_SEC, MAX_PX_PER_SEC);
        let anchor_secs = (anchor_ns as f64 / 1_000_000_000.0) as f32;
        let anchor_screen_x = LEFT_PADDING + anchor_secs * old_px - self.scroll_x_px;
        let new_scroll = (LEFT_PADDING + anchor_secs * new_px - anchor_screen_x).max(0.0);
        self.px_per_sec = new_px;
        self.scroll_x_px = new_scroll;
    }
}

/// Alvo identificado por detecção de clique/hover no Canvas da timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    Ruler { time_ns: u64 },
    ClipLeftHandle { track_id: TrackId, clip_id: ClipId },
    ClipRightHandle { track_id: TrackId, clip_id: ClipId },
    ClipBody { track_id: TrackId, clip_id: ClipId },
    Empty,
}

/// Visão normalizada de trilha para cabeçalhos e Canvas (`V1`, `A1`, `T1`).
#[derive(Debug, Clone, Copy)]
pub struct DisplayTrack<'a> {
    pub id: Option<TrackId>,
    pub name: &'a str,
    pub kind: TrackKind,
    pub clips: &'a [Clip],
}

/// Retorna as trilhas reais da timeline garantindo a presença visual de `T1` para títulos.
pub fn display_tracks(timeline: &Timeline) -> Vec<DisplayTrack<'_>> {
    display_tracks_ordered(timeline, false)
}

/// Retorna as trilhas da timeline, opcionalmente destacando `T1` (`Overlay`) no topo.
pub fn display_tracks_ordered(timeline: &Timeline, overlay_first: bool) -> Vec<DisplayTrack<'_>> {
    let mut tracks: Vec<DisplayTrack<'_>> = timeline
        .tracks
        .iter()
        .map(|track| DisplayTrack {
            id: Some(track.id),
            name: track.name.as_str(),
            kind: track.kind,
            clips: &track.clips,
        })
        .collect();

    if !tracks.iter().any(|track| track.kind == TrackKind::Overlay) {
        tracks.push(DisplayTrack {
            id: None,
            name: "T1",
            kind: TrackKind::Overlay,
            clips: &[],
        });
    }
    if overlay_first {
        tracks.sort_by_key(|track| match track.kind {
            TrackKind::Overlay => 0,
            TrackKind::Video => 1,
            TrackKind::Audio => 2,
        });
    }
    tracks
}

/// Converte um instante em nanossegundos para coordenada horizontal `x` em pixels.
pub fn time_ns_to_x(time_ns: u64, px_per_sec: f32, scroll_x_px: f32) -> f32 {
    let secs = (time_ns as f64) / 1_000_000_000.0;
    (f64::from(LEFT_PADDING) + secs * f64::from(px_per_sec) - f64::from(scroll_x_px)) as f32
}

/// Converte coordenada horizontal `x` em pixels para instante em nanossegundos.
pub fn x_to_time_ns(x: f32, px_per_sec: f32, scroll_x_px: f32) -> u64 {
    if !x.is_finite() || !px_per_sec.is_finite() || px_per_sec <= 0.0 {
        return 0;
    }
    let shifted = f64::from(x) - f64::from(LEFT_PADDING) + f64::from(scroll_x_px);
    if shifted <= 0.0 {
        return 0;
    }
    ((shifted / f64::from(px_per_sec)) * 1_000_000_000.0).round() as u64
}

/// Escolhe o intervalo entre marcações principais da régua mantendo espaçamento legível.
pub fn ruler_step_secs(px_per_sec: f32) -> u64 {
    const STEPS: [u64; 7] = [1, 2, 5, 10, 15, 30, 60];
    let safe_px = px_per_sec.max(MIN_PX_PER_SEC);
    STEPS
        .into_iter()
        .find(|&step| (step as f32) * safe_px >= 80.0)
        .unwrap_or(60)
}

/// Formata segundos inteiros para marcação de régua (`MM:SS`).
pub fn format_ruler_label(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Detecta o elemento sob a coordenada `point` dentro da área do `Canvas`.
pub fn hit_test(point: Point, tracks: &[DisplayTrack<'_>], px_per_sec: f32, scroll_x_px: f32) -> HitTarget {
    if !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0 {
        return HitTarget::Empty;
    }
    if point.y < RULER_HEIGHT {
        return HitTarget::Ruler {
            time_ns: x_to_time_ns(point.x, px_per_sec, scroll_x_px),
        };
    }

    let rel_y = point.y - RULER_HEIGHT;
    let row = (rel_y / TRACK_STRIDE).floor() as usize;
    let in_row_y = rel_y - (row as f32) * TRACK_STRIDE;
    if in_row_y > TRACK_HEIGHT {
        return HitTarget::Empty;
    }

    let Some(track) = tracks.get(row) else {
        return HitTarget::Empty;
    };
    let Some(track_id) = track.id else {
        return HitTarget::Empty;
    };

    for clip in track.clips {
        let left_x = time_ns_to_x(clip.timeline_start_ns, px_per_sec, scroll_x_px);
        let end_ns = clip.timeline_start_ns.saturating_add(clip.duration_ns);
        let right_x = time_ns_to_x(end_ns, px_per_sec, scroll_x_px).max(left_x + 12.0);
        if point.x >= left_x && point.x <= right_x {
            let width = right_x - left_x;
            let handle_w = TRIM_HANDLE_WIDTH.min(width * 0.30);
            if point.x <= left_x + handle_w {
                return HitTarget::ClipLeftHandle { track_id, clip_id: clip.id };
            }
            if point.x >= right_x - handle_w {
                return HitTarget::ClipRightHandle { track_id, clip_id: clip.id };
            }
            return HitTarget::ClipBody { track_id, clip_id: clip.id };
        }
    }

    HitTarget::Empty
}

/// Coleta pontos magnéticos relevantes (origem, agulha e bordas dos demais clipes).
pub fn collect_snap_targets(timeline: &Timeline, playhead_ns: u64, ignore_clip: Option<ClipId>) -> Vec<u64> {
    let mut targets = vec![0, playhead_ns];
    for track in &timeline.tracks {
        for clip in &track.clips {
            if Some(clip.id) == ignore_clip {
                continue;
            }
            targets.push(clip.timeline_start_ns);
            targets.push(clip.timeline_start_ns.saturating_add(clip.duration_ns));
        }
    }
    targets
}

/// Ajusta `candidate_ns` para o alvo mais próximo se estiver dentro do limiar em pixels.
pub fn snap_time_ns(candidate_ns: u64, targets: &[u64], px_per_sec: f32, snap_enabled: bool) -> u64 {
    if !snap_enabled || !px_per_sec.is_finite() || px_per_sec <= 0.0 {
        return candidate_ns;
    }
    let threshold_ns = ((f64::from(SNAP_THRESHOLD_PX) / f64::from(px_per_sec)) * 1_000_000_000.0).round() as u64;
    let mut best = None;
    for &target in targets {
        let diff = candidate_ns.abs_diff(target);
        if diff <= threshold_ns && best.is_none_or(|(_, best_diff)| diff < best_diff) {
            best = Some((target, diff));
        }
    }
    best.map_or(candidate_ns, |(target, _)| target)
}

/// Calcula novo início, entrada na origem e duração ao arrastar a alça esquerda do clipe.
pub fn compute_left_trim(clip: &Clip, delta_ns: i64) -> (u64, u64, u64) {
    if delta_ns < 0 {
        let extend = delta_ns.unsigned_abs().min(clip.timeline_start_ns.min(clip.source_in_ns));
        (
            clip.timeline_start_ns - extend,
            clip.source_in_ns - extend,
            clip.duration_ns.saturating_add(extend),
        )
    } else {
        let max_trim = clip.duration_ns.saturating_sub(MIN_CLIP_DURATION_NS);
        let trim = (delta_ns as u64).min(max_trim);
        (
            clip.timeline_start_ns.saturating_add(trim),
            clip.source_in_ns.saturating_add(trim),
            clip.duration_ns - trim,
        )
    }
}

/// Calcula nova duração ao arrastar a alça direita do clipe respeitando o limite da mídia.
pub fn compute_right_trim(clip: &Clip, delta_ns: i64, max_source_ns: Option<u64>) -> u64 {
    let raw = if delta_ns >= 0 {
        clip.duration_ns.saturating_add(delta_ns as u64)
    } else {
        clip.duration_ns.saturating_sub(delta_ns.unsigned_abs())
    };
    let mut clamped = raw.max(MIN_CLIP_DURATION_NS);
    if let Some(max_src) = max_source_ns {
        if max_src > clip.source_in_ns {
            let max_dur = (max_src - clip.source_in_ns).max(MIN_CLIP_DURATION_NS);
            clamped = clamped.min(max_dur);
        }
    }
    clamped
}

/// Converte deslocamento horizontal em pixels para delta com sinal em nanossegundos.
pub fn dx_to_delta_ns(dx: f32, px_per_sec: f32) -> i64 {
    if !dx.is_finite() || !px_per_sec.is_finite() || px_per_sec <= 0.0 {
        return 0;
    }
    ((f64::from(dx) / f64::from(px_per_sec)) * 1_000_000_000.0).round() as i64
}

/// Soma um delta com sinal a um instante base em nanossegundos com saturação em zero.
pub fn apply_signed_delta(base: u64, delta: i64) -> u64 {
    if delta >= 0 {
        base.saturating_add(delta as u64)
    } else {
        base.saturating_sub(delta.unsigned_abs())
    }
}
