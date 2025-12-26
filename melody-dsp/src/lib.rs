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
}

#[wasm_bindgen]
impl MelodyShifter {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32) -> MelodyShifter {
        // delay-line pitch shifter: 40ms程度のディレイバッファ
        let mut max_delay = (sample_rate * 0.04).round() as usize;
        max_delay = max_delay.clamp(256, 16384);

        MelodyShifter {
            sample_rate,
            max_delay,
            buffer: vec![0.0; max_delay],
            write_idx: 0,
            delay_pos: 0.0,
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
        let mut ratio = (2.0_f32).powf(semitones / 12.0);
        if !ratio.is_finite() || ratio <= 0.0 {
            ratio = 1.0;
        }
        // 極端な値は暴れるので軽く制限
        ratio = ratio.clamp(0.5, 2.0);

        let bypass = semitones.abs() < 1.0e-3 || (ratio - 1.0).abs() < 1.0e-3;
        let len = self.max_delay as f32;
        let half = len * 0.5;

        for x in input.iter_mut() {
            let in_sample = *x;

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

#[derive(Clone, Debug)]
struct NoteSpan {
    start: f32,
    end: f32,

    // absolute pitch reference (MIDI note number)
    base_semitone: f32,

    // pitch (semitones)
    pitch_offset: f32,
    pitch_center_offset: f32,

    // 0..2 (UI側でクランプしているが念のため)
    pitch_mod_amount: f32,
    pitch_drift_amount: f32,

    // timing (not applied yet)
    time_stretch_start: f32,
    time_stretch_end: f32,

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
    shifter: MelodyShifter,
    harmonic_eq: HarmonicEQ,

    // stateful timbre processing to avoid clicks at block boundaries
    timbre_active_note_idx: Option<usize>,
    timbre_filters: Vec<Biquad>,
    timbre_lp: f32,
    timbre_last_f0: f32,
}

#[wasm_bindgen]
impl MelodyEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32) -> MelodyEngine {
        MelodyEngine {
            sample_rate,
            notes: Vec::new(),
            shifter: MelodyShifter::new(sample_rate),
            harmonic_eq: HarmonicEQ::new(),

            timbre_active_note_idx: None,
            timbre_filters: Vec::new(),
            timbre_lp: 0.0,
            timbre_last_f0: 0.0,
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

    /// ノート情報をセットする。
    /// - note_starts / note_ends: 秒
    /// - note_offsets: 半音（+で高く、-で低く）
    /// - pitch_center_offsets: 半音（ピッチセンター）
    /// - pitch_mod_amounts / pitch_drift_amounts: 0..2（量）
    /// - time_stretch_starts / time_stretch_ends: 0.5..2.0（倍率、現状未適用）
    /// - formant_shifts: 半音（現状未適用）
    #[wasm_bindgen]
    pub fn set_notes(
        &mut self,
        note_starts: Vec<f32>,
        note_ends: Vec<f32>,
        base_semitones: Vec<f32>,
        note_offsets: Vec<f32>,
        pitch_center_offsets: Vec<f32>,
        pitch_mod_amounts: Vec<f32>,
        pitch_drift_amounts: Vec<f32>,
        time_stretch_starts: Vec<f32>,
        time_stretch_ends: Vec<f32>,
        formant_shifts: Vec<f32>,
        harmonics_per_note: u32,
        note_harmonics_flat: Vec<f32>,
    ) {
        let n = note_starts
            .len()
            .min(note_ends.len())
            .min(base_semitones.len())
            .min(note_offsets.len())
            .min(pitch_center_offsets.len())
            .min(pitch_mod_amounts.len())
            .min(pitch_drift_amounts.len())
            .min(time_stretch_starts.len())
            .min(time_stretch_ends.len())
            .min(formant_shifts.len());

        self.notes.clear();
        self.notes.reserve(n);

        for i in 0..n {
            let s = note_starts[i];
            let e = note_ends[i];
            let base = base_semitones[i];
            let o = note_offsets[i];
            let pc = pitch_center_offsets[i];
            let pm = pitch_mod_amounts[i];
            let pd = pitch_drift_amounts[i];
            let ts_s = time_stretch_starts[i];
            let ts_e = time_stretch_ends[i];
            let f = formant_shifts[i];

            if !s.is_finite()
                || !e.is_finite()
                || !base.is_finite()
                || !o.is_finite()
                || !pc.is_finite()
                || !pm.is_finite()
                || !pd.is_finite()
                || !ts_s.is_finite()
                || !ts_e.is_finite()
                || !f.is_finite()
            {
                continue;
            }
            if e <= s {
                continue;
            }

            let clamp_amount_0_2 = |v: f32| v.max(0.0).min(2.0);
            let clamp_stretch_05_2 = |v: f32| v.max(0.5).min(2.0);

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
                start: s.max(0.0),
                end: e.max(0.0),
                base_semitone: base,
                pitch_offset: o,
                pitch_center_offset: pc,
                pitch_mod_amount: clamp_amount_0_2(pm),
                pitch_drift_amount: clamp_amount_0_2(pd),
                time_stretch_start: clamp_stretch_05_2(ts_s),
                time_stretch_end: clamp_stretch_05_2(ts_e),
                formant_shift: f,
                harmonic_profile: profile,
            });
        }

        // まずは単純に start でソート（重なりや包含は未定義）
        self.notes.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));

        // reset timbre state (note indices/profiles may have changed)
        self.timbre_active_note_idx = None;
        self.timbre_filters.clear();
        self.timbre_lp = 0.0;
        self.timbre_last_f0 = 0.0;
    }

    /// input(モノラル)をノート配列に従って in-place で処理する。
    ///
    /// 「属するノートがあればそのoffsetでピッチシフト、なければバイパス」という仕様。
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

        // ノート内パラメータを「時間で変化するピッチ」に反映するため、
        // note.end だけでなく固定ブロックで区切って shifter に渡す。
        const BLOCK_SAMPLES: usize = 128;
        const MOD_HZ: f32 = 5.5;
        const MOD_AMP_SEMI: f32 = 0.25;
        const DRIFT_AMP_SEMI: f32 = 0.2;
        const TIME_RAMP_BASE_SEC: f32 = 0.03;

        // 区間ごとに処理：ノート境界で slice を切り替える
        let mut sample_idx: usize = 0;
        let mut note_idx: usize = 0;

        while sample_idx < input.len() {
            let t = (sample_idx as f32) / sr;

            // t より前のノートを前進して捨てる
            while note_idx < self.notes.len() && t >= self.notes[note_idx].end {
                note_idx += 1;
            }

            // 現在時刻がノート内かどうか
            let (offset, next_boundary_time, active_note_idx) = if note_idx < self.notes.len() {
                let note = &self.notes[note_idx];
                if t >= note.start && t < note.end {
                    // ブロック内は一定オフセットとして近似
                    let block_end_sample = (sample_idx + BLOCK_SAMPLES).min(input.len());
                    let block_end_time = (block_end_sample as f32) / sr;
                    let next_time = note.end.min(block_end_time);

                    let mid_sample = sample_idx as f32 + ((block_end_sample - sample_idx) as f32) * 0.5;
                    let t_mid = mid_sample / sr;
                    let dur = (note.end - note.start).max(1.0e-6);
                    let mut u = (t_mid - note.start) / dur;
                    if u < 0.0 {
                        u = 0.0;
                    }
                    if u > 1.0 {
                        u = 1.0;
                    }

                    let center = note.pitch_offset + note.pitch_center_offset;
                    let mod_part = (2.0 * PI * MOD_HZ * (t_mid - note.start)).sin()
                        * (MOD_AMP_SEMI * note.pitch_mod_amount);
                    let drift_part = (u - 0.5) * 2.0 * (DRIFT_AMP_SEMI * note.pitch_drift_amount);

                    // time-tool の簡易実装：ノート頭/尻で補正量をランプさせる
                    // （バッファ長は変えず、アタック/リリースの“タイミング感”だけ反映）
                    let ramp_s = TIME_RAMP_BASE_SEC * note.time_stretch_start;
                    let ramp_e = TIME_RAMP_BASE_SEC * note.time_stretch_end;
                    let ramp_s = ramp_s.min(dur * 0.45).max(0.0);
                    let ramp_e = ramp_e.min(dur * 0.45).max(0.0);

                    let mut env: f32 = 1.0_f32;
                    if ramp_s > 0.0_f32 {
                        let a = (t_mid - note.start) / ramp_s;
                        env = env.min(a.max(0.0_f32).min(1.0_f32));
                    }
                    if ramp_e > 0.0_f32 {
                        let b = (note.end - t_mid) / ramp_e;
                        env = env.min(b.max(0.0_f32).min(1.0_f32));
                    }

                    (
                        (center + mod_part + drift_part) * env,
                        next_time,
                        Some(note_idx),
                    )
                } else {
                    // 次のノート開始までバイパス（こちらも大きすぎないように固定ブロックで刻む）
                    let block_end_sample = (sample_idx + BLOCK_SAMPLES).min(input.len());
                    let block_end_time = (block_end_sample as f32) / sr;
                    (0.0, note.start.min(block_end_time), None)
                }
            } else {
                // 以降はノートなし
                let block_end_sample = (sample_idx + BLOCK_SAMPLES).min(input.len());
                (0.0, (block_end_sample as f32) / sr, None)
            };

            let mut end_sample = (next_boundary_time * sr).ceil() as isize;
            if end_sample < 0 {
                end_sample = 0;
            }
            let end_sample = (end_sample as usize).min(input.len());
            let end_sample = end_sample.max(sample_idx + 1);

            let slice = &mut input[sample_idx..end_sample];
            self.shifter.process_block(slice, offset);

            // Apply simple timbre shaping (harmonics + formant) for this note block.
            if let Some(nidx) = active_note_idx {
                // note changes => reset state to avoid carrying filter memories across notes
                if self.timbre_active_note_idx != Some(nidx) {
                    self.timbre_active_note_idx = Some(nidx);
                    self.timbre_filters.clear();
                    self.timbre_lp = 0.0;
                    self.timbre_last_f0 = 0.0;
                }

                let note = &self.notes[nidx];
                apply_harmonic_and_formant_stateful(
                    slice,
                    sr,
                    &self.harmonic_eq,
                    note,
                    &mut self.timbre_filters,
                    &mut self.timbre_lp,
                    &mut self.timbre_last_f0,
                );
            }

            sample_idx = end_sample;
        }
    }

    #[wasm_bindgen(getter)]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
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

    // --- Harmonic EQ (very rough): filter bank around n*f0, then mix back.
    // Use absolute pitch (base + center) as f0 reference.
    let f0_midi = note.base_semitone + note.pitch_center_offset;
    let f0 = midi_to_hz(f0_midi);
    if !f0.is_finite() || f0 <= 0.0 {
        return;
    }

    // If f0 changed a lot, rebuild filters (and reset their states).
    let rel_change = if (*last_f0).is_finite() && *last_f0 > 0.0 {
        ((f0 - *last_f0) / *last_f0).abs()
    } else {
        1.0
    };

    let nyq = sr * 0.5;
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

    // --- Formant shift (very rough): spectral tilt using 1-pole lowpass split.
    // Positive formant_shift => brighter; negative => darker.
    let s = note.formant_shift;
    if s.is_finite() && s.abs() > 1.0e-3 {
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
