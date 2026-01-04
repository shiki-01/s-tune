import type { NoteSegment } from './note-model';
import { snapMidiToScale, type KeyScale } from './scale';

export type TimeGridSettings = {
	bpm: number;
	division: 4 | 8 | 16; // quarter, eighth, sixteenth
};

function clamp01(v: number): number {
	if (!Number.isFinite(v)) return 0;
	return Math.max(0, Math.min(1, v));
}

function percentToUnit(amount: number): number {
	// UI is 0..100 (%). Keep it forgiving.
	if (!Number.isFinite(amount)) return 0;
	if (amount <= 1) return clamp01(amount);
	return clamp01(amount / 100);
}

function nearestGridTime(tSec: number, grid: TimeGridSettings): number {
	const bpm = Number.isFinite(grid.bpm) && grid.bpm > 0 ? grid.bpm : 120;
	const division = grid.division;
	const beatSec = 60 / bpm; // quarter note
	const stepSec = beatSec * (4 / division);
	if (!Number.isFinite(stepSec) || stepSec <= 0) return tSec;
	return Math.round(tSec / stepSec) * stepSec;
}

export function applyCorrectPitchMacro(notes: NoteSegment[], amount: number, key: KeyScale): NoteSegment[] {
	const a = percentToUnit(amount);
	if (a <= 0) return notes;

	return notes.map((n) => {
		const current = n.baseSemitone + n.pitchCenterOffset;
		// Always derive target from the current pitch against the active key/scale.
		// This keeps behavior stable even if snappedSemitone is stale.
		const target = snapMidiToScale(current, key);
		const diff = target - current;
		const newCenter = current + diff * a;
		return { ...n, pitchCenterOffset: newCenter - n.baseSemitone, snappedSemitone: target };
	});
}

export function applyReduceDriftMacro(notes: NoteSegment[], amount: number): NoteSegment[] {
	const a = percentToUnit(amount);
	if (a <= 0) return notes;
	return notes.map((n) => ({ ...n, pitchDrift: n.pitchDrift * (1 - a) }));
}

export function applyQuantizeTimeMacro(
	notes: NoteSegment[],
	amount: number,
	grid: TimeGridSettings
): NoteSegment[] {
	const a = percentToUnit(amount);
	if (a <= 0) return notes;

	return notes.map((n) => {
		const t = n.startTime;
		const g = nearestGridTime(t, grid);
		const newStart = t + (g - t) * a;
		const dt = newStart - t;
		return { ...n, startTime: newStart, endTime: n.endTime + dt };
	});
}
