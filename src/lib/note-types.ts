import { v4 as uuidv4 } from 'uuid';

export interface NoteSegment {
	id: string;
	startTime: number; // seconds
	endTime: number; // seconds
	baseSemitone: number; // e.g. MIDI note number (placeholder for later F0)
	pitchOffset: number; // semitones
	enabled: boolean;
}

export interface NoteTrack {
	sampleRate: number;
	duration: number; // seconds
	notes: NoteSegment[];
}

export function createNoteSegment(init: Omit<NoteSegment, 'id'> & { id?: string }): NoteSegment {
	return {
		id: init.id ?? uuidv4(),
		startTime: init.startTime,
		endTime: init.endTime,
		baseSemitone: init.baseSemitone,
		pitchOffset: init.pitchOffset,
		enabled: init.enabled
	};
}
