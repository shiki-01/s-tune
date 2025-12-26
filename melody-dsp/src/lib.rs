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

#[derive(Clone, Copy, Debug)]
struct NoteSpan {
    start: f32,
    end: f32,

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
}

#[wasm_bindgen]
impl MelodyEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32) -> MelodyEngine {
        MelodyEngine {
            sample_rate,
            notes: Vec::new(),
            shifter: MelodyShifter::new(sample_rate),
        }
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
        note_offsets: Vec<f32>,
        pitch_center_offsets: Vec<f32>,
        pitch_mod_amounts: Vec<f32>,
        pitch_drift_amounts: Vec<f32>,
        time_stretch_starts: Vec<f32>,
        time_stretch_ends: Vec<f32>,
        formant_shifts: Vec<f32>,
    ) {
        let n = note_starts
            .len()
            .min(note_ends.len())
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
            let o = note_offsets[i];
            let pc = pitch_center_offsets[i];
            let pm = pitch_mod_amounts[i];
            let pd = pitch_drift_amounts[i];
            let ts_s = time_stretch_starts[i];
            let ts_e = time_stretch_ends[i];
            let f = formant_shifts[i];

            if !s.is_finite()
                || !e.is_finite()
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
            self.notes.push(NoteSpan {
                start: s.max(0.0),
                end: e.max(0.0),
                pitch_offset: o,
                pitch_center_offset: pc,
                pitch_mod_amount: clamp_amount_0_2(pm),
                pitch_drift_amount: clamp_amount_0_2(pd),
                time_stretch_start: clamp_stretch_05_2(ts_s),
                time_stretch_end: clamp_stretch_05_2(ts_e),
                formant_shift: f,
            });
        }

        // まずは単純に start でソート（重なりや包含は未定義）
        self.notes.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
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
            let (offset, next_boundary_time) = if note_idx < self.notes.len() {
                let note = self.notes[note_idx];
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

                    ((center + mod_part + drift_part) * env, next_time)
                } else {
                    // 次のノート開始までバイパス（こちらも大きすぎないように固定ブロックで刻む）
                    let block_end_sample = (sample_idx + BLOCK_SAMPLES).min(input.len());
                    let block_end_time = (block_end_sample as f32) / sr;
                    (0.0, note.start.min(block_end_time))
                }
            } else {
                // 以降はノートなし
                let block_end_sample = (sample_idx + BLOCK_SAMPLES).min(input.len());
                (0.0, (block_end_sample as f32) / sr)
            };

            let mut end_sample = (next_boundary_time * sr).ceil() as isize;
            if end_sample < 0 {
                end_sample = 0;
            }
            let end_sample = (end_sample as usize).min(input.len());
            let end_sample = end_sample.max(sample_idx + 1);

            self.shifter
                .process_block(&mut input[sample_idx..end_sample], offset);

            sample_idx = end_sample;
        }
    }

    #[wasm_bindgen(getter)]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}
