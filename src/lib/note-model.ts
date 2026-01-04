export type { NoteSegment, NoteTrack } from './note-types';
export type { NoteSegmentInit } from './note-types';
export { createNoteSegment } from './note-types';

import type { NoteSegment } from './note-types';

export function getSelectedOrAllNotes(notes: NoteSegment[], selectedNoteIds: string[]): NoteSegment[] {
	if (!Array.isArray(selectedNoteIds) || selectedNoteIds.length === 0) return notes;
	const set = new Set(selectedNoteIds);
	return notes.filter((n) => set.has(n.id));
}
