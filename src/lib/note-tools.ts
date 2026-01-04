import { writable } from 'svelte/store';

export type NoteEditorTool = 'pitch' | 'timing' | 'drift' | 'select' | 'pen' | 'erase';

// Global tool state so other panels can follow the current mode.
export const activeNoteEditorTool = writable<NoteEditorTool>('pitch');
