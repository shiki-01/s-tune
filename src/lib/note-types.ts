import { v4 as uuidv4 } from 'uuid';

export interface NoteSegment {
	id: string;
	startTime: number; // seconds
	endTime: number; // seconds
	baseSemitone: number; // e.g. MIDI note number (placeholder for later F0)
	pitchOffset: number; // semitones
	enabled: boolean;

	// tuning (Melodyne-like)
	pitchCenterOffset: number; // semitones
	pitchModAmount: number; // 0..2 (0 = flat)
	pitchDriftAmount: number; // 0..2 (0 = no drift)

	// timing (prep)
	timeStretchStart: number; // 1.0 = original
	timeStretchEnd: number; // 1.0 = original

	// formant (prep)
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
	baseSemitone: number;
	pitchOffset?: number;
	enabled?: boolean;
	pitchCenterOffset?: number;
	pitchModAmount?: number;
	pitchDriftAmount?: number;
	timeStretchStart?: number;
	timeStretchEnd?: number;
	formantShift?: number;
};

export function createNoteSegment(init: NoteSegmentInit): NoteSegment {
	return {
		id: init.id ?? uuidv4(),
		startTime: init.startTime,
		endTime: init.endTime,
		baseSemitone: init.baseSemitone,
		pitchOffset: init.pitchOffset ?? 0,
		enabled: init.enabled ?? true,
		pitchCenterOffset: init.pitchCenterOffset ?? 0,
		pitchModAmount: init.pitchModAmount ?? 1,
		pitchDriftAmount: init.pitchDriftAmount ?? 1,
		timeStretchStart: init.timeStretchStart ?? 1,
		timeStretchEnd: init.timeStretchEnd ?? 1,
		formantShift: init.formantShift ?? 0
	};
}
