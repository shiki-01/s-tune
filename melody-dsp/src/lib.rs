use wasm_bindgen::prelude::*;

use std::f32::consts::PI;

#[wasm_bindgen(start)]
pub fn wasm_start() {
    // ブラウザのコンソールにpanicを出しやすくする
    console_error_panic_hook::set_once();
}

/// 「Melodyneライト」土台：後でF0やノート列を入れられるように、
/// まずは固定半音のブロック処理だけを提供。
#[wasm_bindgen]
pub struct MelodyShifter {
    sample_rate: f32,
    max_delay: usize,
    buffer: Vec<f32>,
    write_idx: usize,
    delay_pos: f32,

    // Smooth ratio changes to avoid clicks when semitones changes abruptly.
    ratio: f32,
    ratio_alpha: f32,
}

#[wasm_bindgen]
impl MelodyShifter {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32) -> MelodyShifter {
        // delay-line pitch shifter: 40ms程度のディレイバッファ
        let mut max_delay = (sample_rate * 0.04).round() as usize;
        max_delay = max_delay.clamp(256, 16384);

        // 10ms smoothing is a good tradeoff for click reduction without feeling laggy.
        let tau_sec = 0.010_f32;
        let ratio_alpha = if sample_rate.is_finite() && sample_rate > 0.0 {
            // alpha = 1 - exp(-1/(sr*tau))
            1.0 - (-1.0 / (sample_rate * tau_sec)).exp()
        } else {
            1.0
        };

        MelodyShifter {
            sample_rate,
            max_delay,
            buffer: vec![0.0; max_delay],
            write_idx: 0,
            delay_pos: 0.0,

            ratio: 1.0,
            ratio_alpha: ratio_alpha.max(0.0).min(1.0),
        }
    }

    /// input(モノラル)を **in-place** にピッチシフトする（オフライン寄り）。
    ///
    /// - input: Float32Array 相当（JSから渡す）
    /// - semitones: +12で1オクターブ上、-12で1オクターブ下
    #[wasm_bindgen]
    pub fn process_block(&mut self, input: &mut [f32], semitones: f32) {
        if input.is_empty() {
            return;
        }

        // semitones + は高く、- は低く
        let mut target_ratio = (2.0_f32).powf(semitones / 12.0);
        if !target_ratio.is_finite() || target_ratio <= 0.0 {
            target_ratio = 1.0;
        }
        // 極端な値は暴れるので軽く制限
        target_ratio = target_ratio.clamp(0.5, 2.0);

        let len = self.max_delay as f32;
        let half = len * 0.5;

        for x in input.iter_mut() {
            let in_sample = *x;

            // Smooth ratio changes sample-by-sample.
            self.ratio += (target_ratio - self.ratio) * self.ratio_alpha;
            let ratio = self.ratio;
            let bypass = (ratio - 1.0).abs() < 1.0e-3;

            // write
            self.buffer[self.write_idx] = in_sample;

            let out_sample = if bypass {
                in_sample
            } else {
                // 2-tap crossfade delay pitch shifter (Bernsee系)
                let d1 = self.delay_pos;
                let mut d2 = d1 + half;
                if d2 >= len {
                    d2 -= len;
                }

                let y1 = read_delay_interp(&self.buffer, self.write_idx, d1);
                let y2 = read_delay_interp(&self.buffer, self.write_idx, d2);

                let fade = 0.5 - 0.5 * (2.0 * PI * d1 / len).cos();
                y1 * fade + y2 * (1.0 - fade)
            };

            *x = out_sample;

            // advance
            self.write_idx += 1;
            if self.write_idx >= self.max_delay {
                self.write_idx = 0;
            }

            // delay pos update: read speed = 1 + (ratio - 1) => ratio
            self.delay_pos += 1.0 - ratio;
            while self.delay_pos < 0.0 {
                self.delay_pos += len;
            }
            while self.delay_pos >= len {
                self.delay_pos -= len;
            }
        }
    }

    #[wasm_bindgen(getter)]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

fn read_delay_interp(buffer: &[f32], write_idx: usize, delay: f32) -> f32 {
    let len = buffer.len() as f32;
    if len <= 1.0 {
        return 0.0;
    }

    let mut pos = (write_idx as f32) - delay;
    pos = pos % len;
    if pos < 0.0 {
        pos += len;
    }

    let i0 = pos.floor() as usize;
    let frac = pos - (i0 as f32);
    let i1 = if i0 + 1 >= buffer.len() { 0 } else { i0 + 1 };
    let a = buffer[i0];
    let b = buffer[i1];
    a + (b - a) * frac
}

fn resample_linear(input: &[f32], out_len: usize) -> Vec<f32> {
    if out_len == 0 {
        return Vec::new();
    }
    if input.is_empty() {
        return vec![0.0; out_len];
    }
    if input.len() == 1 {
        return vec![input[0]; out_len];
    }
    if out_len == 1 {
        return vec![input[0]];
    }

    let in_len_f = (input.len() - 1) as f32;
    let out_len_f = (out_len - 1) as f32;
    let mut out: Vec<f32> = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let pos = (i as f32) * (in_len_f / out_len_f);
        let i0 = pos.floor() as usize;
        let frac = pos - (i0 as f32);
        let i1 = (i0 + 1).min(input.len() - 1);
        let a = input[i0];
        let b = input[i1];
        out.push(a + (b - a) * frac);
    }
    out
}

#[derive(Clone, Debug)]
struct NoteSpan {
    // --- source slice (input)
    src_start: f32,
    src_end: f32,

    // --- destination placement (output)
    dst_start: f32,
    dst_end: f32,

    // pitch mapping
    source_semitone: f32, // original/estimated (MIDI-like)
    base_semitone: f32,   // coarse target (MIDI-like)
    pitch_center_offset: f32, // fine (semitones)
    vibrato_depth: f32,       // semitones
    pitch_drift: f32,         // semitones (linear to end)

    // formant (not applied yet)
    formant_shift: f32,

    // per-note harmonic profile (linear gain, harmonic 1..N)
    harmonic_profile: Vec<f32>,
}

struct HarmonicEQ {
    gains: Vec<f32>, // harmonic 1..N => linear gain (1.0 = 0dB)
}

impl HarmonicEQ {
    fn new() -> Self {
        Self { gains: Vec::new() }
    }

    fn gain(&self, idx: usize) -> f32 {
        // idx: 0 => harmonic 1
        self.gains.get(idx).copied().unwrap_or(1.0)
    }
}

struct TrackFormant {
    shift_semitones: f32,
}

impl TrackFormant {
    fn new() -> Self {
        Self { shift_semitones: 0.0 }
    }
}

fn midi_to_hz(midi: f32) -> f32 {
    440.0_f32 * (2.0_f32).powf((midi - 69.0) / 12.0)
}

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn new_bandpass(sr: f32, f0: f32, q: f32) -> Self {
        // RBJ bandpass (constant skirt gain)
        let f0 = f0.max(1.0).min(sr * 0.49);
        let w0 = 2.0 * PI * f0 / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q.max(0.1));

        // constant skirt gain bandpass
        let b0 = alpha;
        let b1 = 0.0;
        let b2 = -alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        // Direct Form II Transposed
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// ノート配列（開始秒/終了秒/半音オフセット）に基づいてバッファを処理するエンジン。
///
/// ここでは「動く・わかりやすい」を優先し、
/// - ノート探索は素朴（時刻→線形/前進）
/// - オフセットが変わる区間ごとに `MelodyShifter` を呼ぶ
/// とする。後でF0やノート編集に発展させやすい構造だけ先に作る。
#[wasm_bindgen]
pub struct MelodyEngine {
    sample_rate: f32,
    notes: Vec<NoteSpan>,
    harmonic_eq: HarmonicEQ,
    track_formant: TrackFormant,
}

#[wasm_bindgen]
impl MelodyEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32) -> MelodyEngine {
        MelodyEngine {
            sample_rate,
            notes: Vec::new(),
            harmonic_eq: HarmonicEQ::new(),

            track_formant: TrackFormant::new(),
        }
    }

    #[wasm_bindgen]
    pub fn set_harmonic_gains(&mut self, gains: Vec<f32>) {
        // linear gain, clamp to a sane range
        let mut out: Vec<f32> = Vec::with_capacity(gains.len());
        for g in gains.into_iter() {
            if !g.is_finite() {
                out.push(1.0);
            } else {
                out.push(g.max(0.0).min(4.0));
            }
        }
        self.harmonic_eq.gains = out;
    }

    #[wasm_bindgen]
    pub fn set_track_formant_shift(&mut self, shift_semitones: f32) {
        if shift_semitones.is_finite() {
            self.track_formant.shift_semitones = shift_semitones.max(-24.0).min(24.0);
        }
    }

    /// ノート情報をセットする。
    /// - src_starts / src_ends: 入力から切り出す区間（秒）
    /// - dst_starts / dst_ends: 出力へ配置する区間（秒）
    /// - source_semitones / base_semitones: 入力→ターゲットの音高（MIDI相当）
    /// - pitch_center_offsets: 半音（微調整、0.01=1cent）
    /// - vibrato_depths: 半音（0=なし）
    /// - pitch_drifts: 半音（ノート末尾へ向けて線形に加算）
    /// - formant_shifts: 半音（現状未適用）
    #[wasm_bindgen]
    pub fn set_notes(
        &mut self,
        src_starts: Vec<f32>,
        src_ends: Vec<f32>,
        dst_starts: Vec<f32>,
        dst_ends: Vec<f32>,
        source_semitones: Vec<f32>,
        base_semitones: Vec<f32>,
        pitch_center_offsets: Vec<f32>,
        vibrato_depths: Vec<f32>,
        pitch_drifts: Vec<f32>,
        formant_shifts: Vec<f32>,
        harmonics_per_note: u32,
        note_harmonics_flat: Vec<f32>,
    ) {
        let n = src_starts
            .len()
            .min(src_ends.len())
            .min(dst_starts.len())
            .min(dst_ends.len())
            .min(source_semitones.len())
            .min(base_semitones.len())
            .min(pitch_center_offsets.len())
            .min(vibrato_depths.len())
            .min(pitch_drifts.len())
            .min(formant_shifts.len());

        self.notes.clear();
        self.notes.reserve(n);

        for i in 0..n {
            let src_s = src_starts[i];
            let src_e = src_ends[i];
            let dst_s = dst_starts[i];
            let dst_e = dst_ends[i];
            let src_midi = source_semitones[i];
            let base_midi = base_semitones[i];
            let pc = pitch_center_offsets[i];
            let vib = vibrato_depths[i];
            let drift = pitch_drifts[i];
            let f = formant_shifts[i];

            if !src_s.is_finite()
                || !src_e.is_finite()
                || !dst_s.is_finite()
                || !dst_e.is_finite()
                || !src_midi.is_finite()
                || !base_midi.is_finite()
                || !pc.is_finite()
                || !vib.is_finite()
                || !drift.is_finite()
                || !f.is_finite()
            {
                continue;
            }
            if src_e <= src_s {
                continue;
            }
            if dst_e <= dst_s {
                continue;
            }

            let clamp_semi = |v: f32| {
                if v.is_finite() {
                    v.max(-24.0).min(24.0)
                } else {
                    0.0
                }
            };

            let hp = harmonics_per_note as usize;
            let mut profile: Vec<f32> = Vec::new();
            if hp > 0 {
                let need = (i + 1) * hp;
                if note_harmonics_flat.len() >= need {
                    profile.reserve(hp);
                    for j in 0..hp {
                        let g = note_harmonics_flat[i * hp + j];
                        profile.push(if g.is_finite() { g.max(0.0).min(4.0) } else { 1.0 });
                    }
                } else {
                    profile = vec![1.0; hp];
                }
            }
            self.notes.push(NoteSpan {
                src_start: src_s.max(0.0),
                src_end: src_e.max(0.0),
                dst_start: dst_s.max(0.0),
                dst_end: dst_e.max(0.0),
                source_semitone: src_midi,
                base_semitone: base_midi,
                pitch_center_offset: clamp_semi(pc),
                vibrato_depth: clamp_semi(vib.max(0.0)),
                pitch_drift: clamp_semi(drift),
                formant_shift: f,
                harmonic_profile: profile,
            });
        }

        // まずは単純に dst_start でソート（重なりや包含は加算合成）
        self.notes
            .sort_by(|a, b| a.dst_start.partial_cmp(&b.dst_start).unwrap_or(std::cmp::Ordering::Equal));

        // note list replaced; no other state to reset
    }

    /// input(モノラル)をノート配列に従って in-place で処理する。
    ///
    /// 現段階の仕様（まずは「触って反映される」優先）:
    /// - 各ノートごとに src_start/src_end から入力スライスを切り出す
    /// - dst_start/dst_end へ配置して加算合成
    /// - 重なりは単純に加算、隙間は無音
    /// - 境界は短いフェードでクリックを抑制
    #[wasm_bindgen]
    pub fn process_buffer(&mut self, input: &mut [f32]) {
        if input.is_empty() {
            return;
        }
        if self.notes.is_empty() {
            return; // 全バイパス
        }

        let sr = self.sample_rate;
        if !sr.is_finite() || sr <= 0.0 {
            return;
        }

        const BLOCK_SAMPLES: usize = 128;
        const VIB_HZ: f32 = 5.5;

        let original: Vec<f32> = input.to_vec();
        // Preserve original where there are no notes, and overwrite (crossfade) where notes exist.
        // This is intentionally "replacement-ish": if notes overlap, later notes win.
        let mut out: Vec<f32> = original.clone();

        let fade_len = ((sr * 0.010).round() as usize).clamp(0, 4096); // 10ms

        for note in self.notes.iter() {
            let src0 = (note.src_start * sr).round() as isize;
            let src1 = (note.src_end * sr).round() as isize;
            let dst0 = (note.dst_start * sr).round() as isize;
            let dst1 = (note.dst_end * sr).round() as isize;

            let len = original.len() as isize;
            let src0 = src0.clamp(0, len);
            let src1 = src1.clamp(0, len);
            let dst0 = dst0.clamp(0, len);
            let dst1 = dst1.clamp(0, len);

            if src1 <= src0 + 1 {
                continue;
            }
            if dst1 <= dst0 + 1 {
                continue;
            }

            let src0u = src0 as usize;
            let src1u = src1 as usize;
            let dst0u = dst0 as usize;
            let dst1u = dst1 as usize;

            let src_slice = &original[src0u..src1u];
            let dst_len = dst1u - dst0u;

            // Time-stretch (very rough): resample source slice to destination length.
            let mut seg: Vec<f32> = if src_slice.len() == dst_len {
                src_slice.to_vec()
            } else {
                resample_linear(src_slice, dst_len)
            };

            if seg.len() < 2 {
                continue;
            }

            // Apply time-varying pitch shift across the note.
            let mut shifter = MelodyShifter::new(sr);
            let dur_sec = (note.dst_end - note.dst_start).max(1.0e-6);
            let base_shift = (note.base_semitone - note.source_semitone) + note.pitch_center_offset;

            let mut idx: usize = 0;
            while idx < seg.len() {
                let end = (idx + BLOCK_SAMPLES).min(seg.len());
                let mid = idx as f32 + ((end - idx) as f32) * 0.5;
                let u = if seg.len() > 1 {
                    (mid / ((seg.len() - 1) as f32)).max(0.0).min(1.0)
                } else {
                    0.0
                };
                let t_rel = u * dur_sec;
                let vib = (2.0 * PI * VIB_HZ * t_rel).sin() * note.vibrato_depth;
                let drift = note.pitch_drift * u;
                let semitones = base_shift + vib + drift;
                shifter.process_block(&mut seg[idx..end], semitones);
                idx = end;
            }

            // Optional timbre shaping (harmonics/formant) using fresh per-note state.
            // This avoids cross-note state bleed when timing shifts or overlaps.
            let mut filters: Vec<Biquad> = Vec::new();
            let mut lp: f32 = 0.0;
            let mut last_f0: f32 = 0.0;
            apply_harmonic_and_formant_stateful(
                &mut seg,
                sr,
                &self.harmonic_eq,
                note,
                &mut filters,
                &mut lp,
                &mut last_f0,
            );

            let fade = fade_len.min(seg.len() / 2);
            for i in 0..seg.len() {
                let mut w = 1.0_f32;
                if fade > 0 {
                    let w_in = (i as f32 / fade as f32).min(1.0);
                    let w_out = ((seg.len() - 1 - i) as f32 / fade as f32).min(1.0);
                    w = w_in.min(w_out);
                }
                let di = dst0u + i;
                if di >= out.len() {
                    break;
                }
                let x = out[di];
                let mut y = x * (1.0 - w) + seg[i] * w;
                if !y.is_finite() {
                    y = 0.0;
                } else {
                    y = y.max(-1.0).min(1.0);
                }
                out[di] = y;
            }
        }

        // Track-level formant shift (very rough): apply a gentle spectral tilt to the whole output.
        apply_track_formant(&mut out, sr, self.track_formant.shift_semitones);

        input.copy_from_slice(&out);
    }

    #[wasm_bindgen(getter)]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

fn apply_track_formant(input: &mut [f32], sr: f32, shift_semitones: f32) {
    if input.is_empty() {
        return;
    }
    if !sr.is_finite() || sr <= 0.0 {
        return;
    }
    if !shift_semitones.is_finite() {
        return;
    }
    let eps = 1.0e-3_f32;
    if shift_semitones.abs() <= eps {
        return;
    }

    let nyq = sr * 0.5;
    let s = shift_semitones.max(-24.0).min(24.0);
    let tilt = (2.0_f32).powf(s / 12.0);
    let gain_hi = tilt.powf(0.5).max(0.5).min(2.0);
    let gain_lo = (1.0 / tilt).powf(0.5).max(0.5).min(2.0);

    // A single split point gives a "brighter/darker" feel without real formant shifting.
    let fc = 900.0_f32.min(nyq * 0.9).max(80.0);
    let a = (-2.0 * PI * fc / sr).exp();
    let mut lp_state: f32 = 0.0;
    for i in 0..input.len() {
        let x = input[i];
        lp_state = a * lp_state + (1.0 - a) * x;
        let low = lp_state;
        let high = x - low;
        let mut y = low * gain_lo + high * gain_hi;
        y = y.tanh();
        input[i] = y.max(-1.0).min(1.0);
    }
}

fn apply_harmonic_and_formant_stateful(
    input: &mut [f32],
    sr: f32,
    global_eq: &HarmonicEQ,
    note: &NoteSpan,
    filters: &mut Vec<Biquad>,
    lp_state: &mut f32,
    last_f0: &mut f32,
) {
    if input.is_empty() {
        return;
    }
    if !sr.is_finite() || sr <= 0.0 {
        return;
    }

    let nyq = sr * 0.5;

    // Bypass when everything is neutral to avoid unnecessary filtering artifacts.
    // Default UI state sets all gains to 1.0 and formant_shift to 0.0.
    let eps = 1.0e-3_f32;
    let formant_active = note.formant_shift.is_finite() && note.formant_shift.abs() > eps;

    let mut harmonic_active = false;
    for &g in global_eq.gains.iter().take(24) {
        if g.is_finite() && (g - 1.0).abs() > eps {
            harmonic_active = true;
            break;
        }
    }
    if !harmonic_active {
        for &g in note.harmonic_profile.iter().take(24) {
            if g.is_finite() && (g - 1.0).abs() > eps {
                harmonic_active = true;
                break;
            }
        }
    }

    if !harmonic_active && !formant_active {
        return;
    }

    // --- Harmonic EQ (very rough): filter bank around n*f0, then mix back.
    // Use absolute pitch (base + center) as f0 reference.
    // Only run if gains are actually non-neutral.
    let mut f0: f32 = 0.0;
    if harmonic_active {
        let f0_midi = note.base_semitone + note.pitch_center_offset;
        f0 = midi_to_hz(f0_midi);
        if !f0.is_finite() || f0 <= 0.0 {
            // If f0 is invalid, skip harmonic EQ but still allow formant.
            harmonic_active = false;
        }
    }

    if harmonic_active {
        // If f0 changed a lot, rebuild filters (and reset their states).
        let rel_change = if (*last_f0).is_finite() && *last_f0 > 0.0 {
            ((f0 - *last_f0) / *last_f0).abs()
        } else {
            1.0
        };

        let n_harm = global_eq.gains.len().max(note.harmonic_profile.len()).min(24);

        // Ensure filters match current f0/harmonic count.
        let desired = (1..=n_harm)
            .map(|h| f0 * (h as f32))
            .take_while(|&f| f.is_finite() && f < nyq * 0.98)
            .count();

        let need_rebuild = filters.len() != desired || rel_change > 0.08;
        if need_rebuild {
            filters.clear();
            let q = 12.0;
            for h in 0..desired {
                let harm_idx = (h + 1) as f32;
                let f = f0 * harm_idx;
                filters.push(Biquad::new_bandpass(sr, f, q));
            }
            *last_f0 = f0;
        }

        // Keep it subtle: this is not a true EQ, it's a rough "timbre feel".
        let mix = 0.25_f32;
        let n_filt = filters.len().max(1) as f32;

        for i in 0..input.len() {
            let x = input[i];
            let mut acc = 0.0_f32;

            for (h, filt) in filters.iter_mut().enumerate() {
                let g_global = global_eq.gain(h);
                let g_note = note.harmonic_profile.get(h).copied().unwrap_or(1.0);
                // Cap per-harmonic gain to avoid blowing up when summing many harmonics.
                let g = (g_global * g_note).max(0.0).min(2.0);
                acc += filt.process(x) * g;
            }

            // Normalize the summed band outputs.
            acc /= n_filt;

            let mut y = x * (1.0 - mix) + acc * mix;
            // Simple limiter/soft clip to prevent hard digital clipping.
            y = y.tanh();
            input[i] = y.max(-1.0).min(1.0);
        }
    }

    // --- Formant shift (very rough): spectral tilt using 1-pole lowpass split.
    // Positive formant_shift => brighter; negative => darker.
    let s = note.formant_shift;
    if formant_active {
        let tilt = (2.0_f32).powf(s / 12.0);
        let gain_hi = tilt.powf(0.5).max(0.5).min(2.0);
        let gain_lo = (1.0 / tilt).powf(0.5).max(0.5).min(2.0);

        let fc = 900.0_f32.min(nyq * 0.9).max(80.0);
        let a = (-2.0 * PI * fc / sr).exp();
        for i in 0..input.len() {
            let x = input[i];
            *lp_state = a * (*lp_state) + (1.0 - a) * x;
            let low = *lp_state;
            let high = x - low;
            let mut y = low * gain_lo + high * gain_hi;
            y = y.tanh();
            input[i] = y.max(-1.0).min(1.0);
        }
    }
}
