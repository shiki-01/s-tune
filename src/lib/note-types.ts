import { v4 as uuidv4 } from 'uuid';

export interface NoteSegment {
	id: string;
	// --- output placement (editable with Timing tool)
	startTime: number; // seconds
	endTime: number; // seconds

	// --- source slice (used by DSP when timing shifts)
	sourceStartTime: number; // seconds
	sourceEndTime: number; // seconds

	// --- pitch
	sourceSemitone: number; // original/estimated (MIDI-like)
	baseSemitone: number; // coarse target (MIDI-like)
	pitchCenterOffset: number; // fine average offset (semitones; 0.01 = 1 cent)
	pitchDrift: number; // semitone drift added linearly toward note end
	vibratoDepth: number; // semitones (0 = none)
	// recommendation / snap (optional)
	snappedSemitone?: number; // recommended pitch (MIDI) in selected key/scale
	enabled: boolean;
	// selection (optional; used by macro UI helpers)
	selected?: boolean;

	// formant (optional; used by SoundEditor)
	formantShift: number; // semitone-ish, 0 = original
}

export interface NoteTrack {
	sampleRate: number;
	duration: number; // seconds
	notes: NoteSegment[];
}

export type NoteSegmentInit = {
	id?: string;
	startTime: number;
	endTime: number;
	sourceStartTime?: number;
	sourceEndTime?: number;
	sourceSemitone?: number;
	baseSemitone: number;
	snappedSemitone?: number;
	enabled?: boolean;
	pitchCenterOffset?: number;
	pitchDrift?: number;
	vibratoDepth?: number;
	formantShift?: number;
	selected?: boolean;
};

export function createNoteSegment(init: NoteSegmentInit): NoteSegment {
	return {
		id: init.id ?? uuidv4(),
		startTime: init.startTime,
		endTime: init.endTime,
		sourceStartTime: init.sourceStartTime ?? init.startTime,
		sourceEndTime: init.sourceEndTime ?? init.endTime,
		sourceSemitone: init.sourceSemitone ?? init.baseSemitone,
		baseSemitone: init.baseSemitone,
		snappedSemitone: init.snappedSemitone ?? init.baseSemitone,
		enabled: init.enabled ?? true,
		pitchCenterOffset: init.pitchCenterOffset ?? 0,
		pitchDrift: init.pitchDrift ?? 0,
		vibratoDepth: init.vibratoDepth ?? 0,
		formantShift: init.formantShift ?? 0,
		selected: init.selected
	};
}
