import type { NoteSegment } from './note-model';
import type { DetectedNote } from './pitch-model';
import type { KeyScale } from './scale';
import { detectedNotesToNoteSegments } from './note-detection';

// Thin wrapper to match requested API name.
export function mapDetectedNotesToSegments(notes: DetectedNote[], key: KeyScale): NoteSegment[] {
	return detectedNotesToNoteSegments(notes, key);
}
