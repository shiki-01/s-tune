export type EditorTool = 'select' | 'pen' | 'erase';

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
