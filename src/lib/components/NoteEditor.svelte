<script lang="ts">
	import type { NoteSegment, NoteTrack } from '$lib/note-model';
	import { createNoteSegment } from '$lib/note-model';

	type Props = {
		track: NoteTrack;
		selectedNoteId: string | null;
		onChange: (next: NoteTrack) => void;
		onSelect: (id: string | null) => void;
	};

	let { track, selectedNoteId, onChange, onSelect }: Props = $props();

	let editor: HTMLDivElement | null = $state(null);
	let newStartTime = $state(0);
	let newEndTime = $state(1);
	let newBaseSemitone = $state(60);
	let newPitchOffset = $state(0);
	let newEnabled = $state(true);
	let selectedNote: NoteSegment | null = $state(null);

	$effect(() => {
		selectedNote = selectedNoteId
			? (track.notes.find((n) => n.id === selectedNoteId) ?? null)
			: null;
	});

	function clampTime(t: number): number {
		if (!Number.isFinite(t)) return 0;
		return Math.max(0, Math.min(track.duration, t));
	}

	function selectNoteByTime(timeSec: number) {
		const t = clampTime(timeSec);
		const hit = track.notes.find((n) => t >= n.startTime && t < n.endTime);
		onSelect(hit?.id ?? null);
	}

	function updateNote(id: string, patch: Partial<NoteSegment>) {
		onChange({
			...track,
			notes: track.notes.map((n) => (n.id === id ? { ...n, ...patch } : n))
		});
	}

	function addNote() {
		const startTime = clampTime(newStartTime);
		const endTime = clampTime(newEndTime);
		if (!(endTime > startTime)) return;

		const seg = createNoteSegment({
			startTime,
			endTime,
			baseSemitone: Number.isFinite(newBaseSemitone) ? newBaseSemitone : 60,
			pitchOffset: Number.isFinite(newPitchOffset) ? newPitchOffset : 0,
			enabled: !!newEnabled
		});

		onChange({
			...track,
			notes: [...track.notes, seg].sort((a, b) => a.startTime - b.startTime)
		});
		onSelect(seg.id);
	}

	function removeSelected() {
		if (!selectedNoteId) return;
		const id = selectedNoteId;
		onChange({ ...track, notes: track.notes.filter((n) => n.id !== id) });
		onSelect(null);
	}

	let W = $derived(() => {
		if (!editor) return 720;
		return editor.clientWidth;
	});
	let H = $derived(() => {
		if (!editor) return 160;
		return editor.clientHeight;
	});
</script>

<div class="w:100% h:100% flex flex:column gap:10px">
	<div class="flex flex:column gap:10px">
		<label class="flex flex:row w:200px jc:space-between ai:center">
			start(s)
			<input
				class="w:100px p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
				type="number"
				step="0.01"
				min="0"
				max={track.duration}
				bind:value={newStartTime}
			/>
		</label>
		<label class="flex flex:row w:200px jc:space-between ai:center">
			end(s)
			<input
				class="w:100px p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
				type="number"
				step="0.01"
				min="0"
				max={track.duration}
				bind:value={newEndTime}
			/>
		</label>
		<label class="flex flex:row w:200px jc:space-between ai:center">
			base(MIDI)
			<input
				class="w:100px p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
				type="number"
				step="1"
				bind:value={newBaseSemitone}
			/>
		</label>
		<label class="flex flex:row w:200px jc:space-between ai:center">
			offset(semi)
			<input
				class="w:100px p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
				type="number"
				step="0.1"
				bind:value={newPitchOffset}
			/>
		</label>
		<label>
			enabled
			<input type="checkbox" bind:checked={newEnabled} />
		</label>
		<div class="flex flex:row gap:10px">
			<button
				class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
				type="button"
				onclick={addNote}>ノート追加</button
			>
			<button
				class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
				type="button"
				onclick={removeSelected}
				disabled={!selectedNoteId}>選択ノート削除</button
			>
		</div>
	</div>

	<div bind:this={editor} class="w:100% h:300px b:2px|solid|#222 r:6px">
		<svg
			class="outline:none:focus"
			width={W()}
			height={H()}
			viewBox={`0 0 ${W()} ${H()}`}
			role="button"
			tabindex="0"
			aria-label="ノートを選択"
			onclick={(e) => {
				const rect = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
				const x = e.clientX - rect.left;
				const t = (x / rect.width) * track.duration;
				selectNoteByTime(t);
			}}
			onkeydown={(e) => {
				if (e.key !== 'Enter' && e.key !== ' ') return;
				e.preventDefault();
				if (!selectedNoteId) selectNoteByTime(0);
			}}
		>
			{#each track.notes as n (n.id)}
				{@const x = (n.startTime / track.duration) * W()}
				{@const w = ((n.endTime - n.startTime) / track.duration) * W()}
				{@const y = 20 + (120 - (n.baseSemitone - 48) * 4)}
				<rect
					{x}
					y={Math.max(10, Math.min(130, y))}
					width={Math.max(2, w)}
					height="18"
					opacity={n.enabled ? 1 : 0.3}
					fill={n.id === selectedNoteId ? 'rgba(0, 120, 255, 0.55)' : 'rgba(0,0,0,0.25)'}
					stroke="rgba(0,0,0,0.35)"
				/>
			{/each}
		</svg>
	</div>

	{#if selectedNote}
		<div class="flex flex:column gap:10px">
			<div>Selected: {selectedNote.startTime.toFixed(2)}s–{selectedNote.endTime.toFixed(2)}s</div>
			<div class="flex flex:row gap:20px">
				<label class="flex flex:row gap:10px ai:center jc:center">
					offset(semi)
					<input
						class="w:100px p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
						type="number"
						step="0.1"
						value={selectedNote.pitchOffset}
						oninput={(e) => {
							if (selectedNote) {
								updateNote(selectedNote.id, {
									pitchOffset: Number((e.currentTarget as HTMLInputElement).value)
								});
							}
						}}
					/>
				</label>
				<label class="flex flex:row gap:10px ai:center jc:center">
					enabled
					<input
						class="w:20px p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
						type="checkbox"
						checked={selectedNote.enabled}
						onchange={(e) => {
							if (selectedNote) {
								updateNote(selectedNote.id, {
									enabled: (e.currentTarget as HTMLInputElement).checked
								});
							}
						}}
					/>
				</label>
			</div>
		</div>
	{/if}
</div>
