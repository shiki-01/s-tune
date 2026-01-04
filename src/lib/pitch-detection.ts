import type { PitchFrame } from './pitch-model';

export interface PitchDetector {
	detectPitch(audioBuffer: AudioBuffer): Promise<PitchFrame[]>;
}

export type DummyPitchDetectorOptions = {
	frameHopSec?: number;
	maxDurationSec?: number;
	baseMidi?: number;
};

function clamp01(x: number): number {
	if (!Number.isFinite(x)) return 0;
	return Math.max(0, Math.min(1, x));
}

function midiToHz(midi: number): number {
	return 440 * Math.pow(2, (midi - 69) / 12);
}

// Requested API: class-based dummy detector (swappable later)
export class DummyPitchDetector implements PitchDetector {
	private frameHopSec: number;
	private maxDurationSec: number;
	private baseMidi: number;

	constructor(options?: DummyPitchDetectorOptions) {
		this.frameHopSec = options?.frameHopSec ?? 0.01;
		this.maxDurationSec = options?.maxDurationSec ?? 20;
		this.baseMidi = options?.baseMidi ?? 60; // C4
	}

	async detectPitch(audioBuffer: AudioBuffer): Promise<PitchFrame[]> {
		const dur = Math.max(0, Math.min(audioBuffer.duration, this.maxDurationSec));
		const total = Math.max(1, Math.floor(dur / this.frameHopSec));

		// 例: 0.35s voiced + 0.10s unvoiced を繰り返しつつ、スケール上を階段状に動く
		const voicedSec = 0.35;
		const unvoicedSec = 0.1;
		const phraseSec = voicedSec + unvoicedSec;

		const scaleSteps = [0, 2, 4, 5, 7, 9, 11] as const; // major
		const frames: PitchFrame[] = [];

		for (let i = 0; i < total; i++) {
			const t = i * this.frameHopSec;
			const phase = t % phraseSec;

			if (phase >= voicedSec) {
				frames.push({ time: t, f0: null, confidence: 0 });
				continue;
			}

			const stepIdx = Math.floor(t / phraseSec) % scaleSteps.length;
			const midi = this.baseMidi + scaleSteps[stepIdx];
			const f0 = midiToHz(midi);

			// 少しだけ揺らぎ (ダミー感を減らす)
			const vibrato = Math.sin(2 * Math.PI * 5 * t) * 0.15; // semitone-ish small
			const f0V = f0 * Math.pow(2, vibrato / 12);

			const conf = clamp01(0.85 - 0.1 * Math.abs(Math.sin(2 * Math.PI * t * 0.7)));
			frames.push({ time: t, f0: f0V, confidence: conf });
		}

		return frames;
	}
}

// Backward-compatible factory
export function createDummyPitchDetector(options?: DummyPitchDetectorOptions): PitchDetector {
	return new DummyPitchDetector(options);
}
