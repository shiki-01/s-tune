use wasm_bindgen::prelude::*;
use pitch_shift::PitchShifter;

#[wasm_bindgen]
pub struct MelodyShifter {
    shifter: PitchShifter,
}

#[wasm_bindgen]
impl MelodyShifter {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32) -> MelodyShifter {
        let shifter = PitchShifter::new(/*window*/ 1024, sample_rate as usize);
        MelodyShifter { shifter }
    }

    #[wasm_bindgen]
    pub fn process_block(&mut self, input: &mut [f32], semitones: f32) {
        // ここでピッチシフトを実行
        self.shifter.set_semitones(semitones);
        self.shifter.process(input);
    }
}
