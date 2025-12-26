import { createNoteSegment, type NoteSegment } from './note-model';
import type { DetectedNote, PitchFrame } from './pitch-model';

export type NoteDetectionConfig = {
	minFrameConfidence: number;
	maxGapSec: number;
	maxJumpSemitones: number;
	maxStdDevSemitones: number;
	minNoteSec: number;
	minFramesPerNote: number;
};

export const NOTE_DETECTION_DEFAULTS: NoteDetectionConfig = {
	minFrameConfidence: 0.3,
	maxGapSec: 0.05,
	maxJumpSemitones: 1.2,
	maxStdDevSemitones: 0.6,
	minNoteSec: 0.06,
	minFramesPerNote: 3
};

function hzToMidiFloat(hz: number): number {
	// MIDI note number (A4=440Hz => 69)
	return 69 + 12 * Math.log2(hz / 440);
}

function median(values: number[]): number {
	if (values.length === 0) return NaN;
	const xs = [...values].sort((a, b) => a - b);
	const mid = Math.floor(xs.length / 2);
	if (xs.length % 2 === 1) return xs[mid]!;
	return (xs[mid - 1]! + xs[mid]!) / 2;
}

function mean(values: number[]): number {
	if (values.length === 0) return NaN;
	return values.reduce((a, b) => a + b, 0) / values.length;
}

function stddev(values: number[]): number {
	if (values.length <= 1) return 0;
	const m = mean(values);
	const v = values.reduce((acc, x) => acc + (x - m) * (x - m), 0) / (values.length - 1);
	return Math.sqrt(v);
}

function inferHopSec(times: number[]): number {
	if (times.length <= 2) return 0.01;
	const deltas: number[] = [];
	for (let i = 1; i < times.length; i++) {
		const d = times[i]! - times[i - 1]!;
		if (Number.isFinite(d) && d > 0) deltas.push(d);
	}
	if (deltas.length === 0) return 0.01;
	return median(deltas);
}

export function detectNotesFromPitch(
	frames: PitchFrame[],
	config?: Partial<NoteDetectionConfig>
): DetectedNote[] {
	const cfg: NoteDetectionConfig = { ...NOTE_DETECTION_DEFAULTS, ...(config ?? {}) };
	const voiced = frames
		.filter((f) => f.f0 != null && Number.isFinite(f.f0) && f.confidence >= cfg.minFrameConfidence)
		.map((f) => ({ time: f.time, midi: hzToMidiFloat(f.f0 as number), confidence: f.confidence }))
		.sort((a, b) => a.time - b.time);

	if (voiced.length === 0) return [];

	const hopSec = inferHopSec(voiced.map((v) => v.time));

	type VFrame = { time: number; midi: number; confidence: number };
	const clusters: VFrame[][] = [];
	let cur: VFrame[] = [];

	const flush = () => {
		if (cur.length === 0) return;
		clusters.push(cur);
		cur = [];
	};

	for (let i = 0; i < voiced.length; i++) {
		const f = voiced[i]!;
		if (cur.length === 0) {
			cur.push(f);
			continue;
		}

		const prev = cur[cur.length - 1]!;
		const gap = f.time - prev.time;
		if (!Number.isFinite(gap) || gap > cfg.maxGapSec) {
			flush();
			cur.push(f);
			continue;
		}

		const jump = Math.abs(f.midi - prev.midi);
		if (jump > cfg.maxJumpSemitones) {
			flush();
			cur.push(f);
			continue;
		}

		// 「追加したら散らばりすぎる」なら分割
		const candidate = [...cur, f];
		const sd = stddev(candidate.map((x) => x.midi));
		if (sd > cfg.maxStdDevSemitones) {
			flush();
			cur.push(f);
			continue;
		}

		cur.push(f);
	}
	flush();

	const notes: DetectedNote[] = [];
	for (const cl of clusters) {
		if (cl.length < cfg.minFramesPerNote) continue;
		const t0 = cl[0]!.time;
		const t1 = cl[cl.length - 1]!.time + hopSec;
		const dur = t1 - t0;
		if (!Number.isFinite(dur) || dur < cfg.minNoteSec) continue;

		const midiRep = Math.round(median(cl.map((x) => x.midi)));
		const conf = Math.max(0, Math.min(1, mean(cl.map((x) => x.confidence))));

		notes.push({
			id: globalThis.crypto?.randomUUID?.() ?? `det-${Math.random().toString(16).slice(2)}`,
			startTime: t0,
			endTime: t1,
			midi: midiRep,
			confidence: conf
		});
	}

	return notes;
}

export type Key = {
	rootMidi: number; // pitch-class is derived from this
	scaleSteps: readonly number[]; // degrees within octave (0..11)
};

export const SCALES = {
	major: [0, 2, 4, 5, 7, 9, 11],
	minor: [0, 2, 3, 5, 7, 8, 10]
} as const;

export type ScaleName = keyof typeof SCALES;

export function makeKey(rootPitchClass: number, scale: ScaleName, rootOctaveMidi = 60): Key {
	const pc = ((rootPitchClass % 12) + 12) % 12;
	return {
		rootMidi: rootOctaveMidi + pc,
		scaleSteps: SCALES[scale]
	};
}

function inScale(midi: number, key: Key): boolean {
	const pc = ((midi - key.rootMidi) % 12 + 12) % 12;
	return key.scaleSteps.includes(pc);
}

export function midiPitchClassName(pc: number): string {
	const names = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'] as const;
	const idx = ((pc % 12) + 12) % 12;
	return names[idx] ?? 'C';
}

export function snapMidiToScale(midi: number, key: Key): number {
	if (!Number.isFinite(midi)) return midi;
	const m = Math.round(midi);
	if (inScale(m, key)) return m;

	// 近傍探索（最大±6半音 = 1オクターブ内）
	let best = m;
	let bestDist = Number.POSITIVE_INFINITY;
	for (let d = 1; d <= 6; d++) {
		const down = m - d;
		const up = m + d;
		if (inScale(down, key)) {
			best = down;
			bestDist = d;
			break;
		}
		if (inScale(up, key)) {
			best = up;
			bestDist = d;
			break;
		}
	}
	return bestDist === Number.POSITIVE_INFINITY ? m : best;
}

export function detectedNotesToNoteSegments(detected: DetectedNote[], key?: Key): NoteSegment[] {
	return detected
		.filter((n) => n.endTime > n.startTime)
		.map((n) => {
			const snapped = key ? snapMidiToScale(n.midi, key) : n.midi;
			return createNoteSegment({
				id: n.id,
				startTime: n.startTime,
				endTime: n.endTime,
				baseSemitone: n.midi,
				pitchOffset: 0,
				pitchCenterOffset: 0,
				pitchModAmount: 1,
				pitchDriftAmount: 1,
				timeStretchStart: 1,
				timeStretchEnd: 1,
				formantShift: 0,
				enabled: true,
				snappedSemitone: snapped
			});
		})
		.sort((a, b) => a.startTime - b.startTime);
}
