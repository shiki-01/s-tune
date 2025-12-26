export type EditorTool =
	| 'select'
	| 'pen'
	| 'erase'
	| 'pitch-center-tool'
	| 'pitch-mod-tool'
	| 'pitch-drift-tool'
	| 'time-tool'
	| 'formant-tool';

export interface EditorState {
	tool: EditorTool;
	selectedNoteIds: string[];
}

export function createEditorState(init?: Partial<EditorState>): EditorState {
	return {
		tool: init?.tool ?? 'select',
		selectedNoteIds: init?.selectedNoteIds ?? []
	};
}
