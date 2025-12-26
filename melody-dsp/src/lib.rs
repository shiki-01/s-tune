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
