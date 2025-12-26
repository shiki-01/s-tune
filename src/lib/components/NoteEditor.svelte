<script lang="ts">
	import type { NoteSegment, NoteTrack } from '$lib/note-model';
	import { createNoteSegment } from '$lib/note-model';
	import { createEditorState, type EditorState, type EditorTool } from '$lib/editor-state';
	import { onDestroy, onMount } from 'svelte';
	import {
		clamp,
		getLocalPoint,
		snapTime,
		timeToX,
		xToTime,
		yToSemitone,
		semitoneToYTop
	} from '$lib/editor/coords';

	type Props = {
		track: NoteTrack;
		selectedNoteIds: string[];
		onChange: (next: NoteTrack) => void;
		onSelect: (ids: string[]) => void;
	};

	let { track, selectedNoteIds, onChange, onSelect }: Props = $props();

	const MIN_SEMITONE = 36; // C2
	const MAX_SEMITONE = 84; // C6
	const ROW_H_PX = 18;
	const GUTTER_PX = 42;
	const EDGE_PX = 6;
	const MIN_NOTE_SEC = 0.02;
	const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'] as const;
	const SVG_H = (MAX_SEMITONE - MIN_SEMITONE + 1) * ROW_H_PX;

	let editor: HTMLDivElement | null = $state(null);
	let svgEl: SVGSVGElement | null = $state(null);
	let svgW = $state(720);

	let editorState = $state(createEditorState({ tool: 'select', selectedNoteIds: [] }));
	let bpm = $state(120);
	let snapEnabled = $state(true);

	$effect(() => {
		editorState.selectedNoteIds = selectedNoteIds;
	});

	$effect(() => {
		if (!editor) return;
		const ro = new ResizeObserver(() => {
			svgW = editor?.clientWidth ?? 720;
		});
		ro.observe(editor);
		svgW = editor.clientWidth;
		return () => ro.disconnect();
	});

	let areaW = $derived(() => getAreaWidth(svgW));
	let beatSec = $derived(() => (Number.isFinite(bpm) && bpm > 0 ? 60 / bpm : 0));

	function setTool(tool: EditorTool) {
		editorState.tool = tool;
	}

	function setSelected(ids: string[]) {
		editorState.selectedNoteIds = ids;
		onSelect(ids);
	}

	function getNoteName(semitone: number): string {
		const idx = ((semitone % 12) + 12) % 12;
		return NOTE_NAMES[idx] ?? 'C';
	}

	function getAreaWidth(totalWidth: number): number {
		return Math.max(1, totalWidth - GUTTER_PX);
	}

	function noteRect(n: NoteSegment) {
		const x0 = timeToX(n.startTime, areaW(), track.duration);
		const x1 = timeToX(n.endTime, areaW(), track.duration);
		const x = GUTTER_PX + x0;
		const w = x1 - x0;
		const h = Math.max(8, ROW_H_PX * 0.85);
		const yTop = semitoneToYTop(n.baseSemitone, SVG_H, MIN_SEMITONE, MAX_SEMITONE);
		const y = yTop + (ROW_H_PX - h) / 2;
		return { x, y, w: Math.max(2, w), h };
	}

	function hitTest(x: number, y: number):
		| { note: NoteSegment; part: 'left' | 'right' | 'body' }
		| null {
		if (x < GUTTER_PX) return null;
		for (let i = track.notes.length - 1; i >= 0; i--) {
			const n = track.notes[i];
			const r = noteRect(n);
			const inside = x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h;
			if (!inside) continue;

			if (x - r.x <= EDGE_PX) return { note: n, part: 'left' };
			if (r.x + r.w - x <= EDGE_PX) return { note: n, part: 'right' };
			return { note: n, part: 'body' };
		}
		return null;
	}

	function updateNotes(patch: (n: NoteSegment) => NoteSegment): void {
		onChange({
			...track,
			notes: track.notes.map((n) => patch(n)).sort((a, b) => a.startTime - b.startTime)
		});
	}

	function removeNote(id: string): void {
		onChange({ ...track, notes: track.notes.filter((n) => n.id !== id) });
		setSelected(editorState.selectedNoteIds.filter((x) => x !== id));
	}

	type DragState =
		| {
			mode: 'pen';
			pointerId: number;
			startTime: number;
			baseSemitone: number;
			previewStart: number;
			previewEnd: number;
		}
		| {
			mode: 'move';
			pointerId: number;
			anchorTime: number;
			anchorSemitone: number;
			snapshot: Map<string, { startTime: number; endTime: number; baseSemitone: number }>;
		}
		| {
			mode: 'resize-left' | 'resize-right';
			pointerId: number;
			anchorTime: number;
			activeId: string;
			snapshot: { startTime: number; endTime: number; baseSemitone: number };
		};

	let drag = $state<DragState | null>(null);

	onMount(() => {
		const onKeyDown = (e: KeyboardEvent) => {
			if (e.key === '1') {
				e.preventDefault();
				setTool('select');
			}
			if (e.key === '2') {
				e.preventDefault();
				setTool('pen');
			}
			if (e.key === '3') {
				e.preventDefault();
				setTool('erase');
			}
		};
		window.addEventListener('keydown', onKeyDown);
		onDestroy(() => window.removeEventListener('keydown', onKeyDown));
	});

	function onPointerDown(e: PointerEvent) {
		if (!svgEl) return;
		svgEl.focus();

		const { x, y, width, height } = getLocalPoint(e, svgEl);
		const areaW = getAreaWidth(width);
		const localX = x - GUTTER_PX;
		const t = snapTime(xToTime(localX, areaW, track.duration), bpm, snapEnabled);
		const semi = yToSemitone(y, height, MIN_SEMITONE, MAX_SEMITONE);
		const hit = hitTest(x, y);
		const multi = e.ctrlKey || e.metaKey;

		if (editorState.tool === 'pen') {
			if (hit) return;
			if (x < GUTTER_PX) return;
			drag = {
				mode: 'pen',
				pointerId: e.pointerId,
				startTime: t,
				baseSemitone: semi,
				previewStart: t,
				previewEnd: t
			};
			svgEl.setPointerCapture(e.pointerId);
			e.preventDefault();
			return;
		}

		if (editorState.tool === 'erase') {
			if (hit) removeNote(hit.note.id);
			e.preventDefault();
			return;
		}

		// select
		if (!hit) {
			if (!multi) setSelected([]);
			return;
		}

		let nextSelected: string[];
		if (multi) {
			const has = editorState.selectedNoteIds.includes(hit.note.id);
			nextSelected = has
				? editorState.selectedNoteIds.filter((id) => id !== hit.note.id)
				: [...editorState.selectedNoteIds, hit.note.id];
		} else {
			nextSelected = [hit.note.id];
		}
		setSelected(nextSelected);

		if (hit.part === 'left' || hit.part === 'right') {
			const active = hit.note;
			drag = {
				mode: hit.part === 'left' ? 'resize-left' : 'resize-right',
				pointerId: e.pointerId,
				anchorTime: t,
				activeId: active.id,
				snapshot: {
					startTime: active.startTime,
					endTime: active.endTime,
					baseSemitone: active.baseSemitone
				}
			};
			svgEl.setPointerCapture(e.pointerId);
			e.preventDefault();
			return;
		}

		const snapMap = new Map<string, { startTime: number; endTime: number; baseSemitone: number }>();
		for (const id of nextSelected) {
			const n = track.notes.find((x) => x.id === id);
			if (!n) continue;
			snapMap.set(id, { startTime: n.startTime, endTime: n.endTime, baseSemitone: n.baseSemitone });
		}

		drag = {
			mode: 'move',
			pointerId: e.pointerId,
			anchorTime: t,
			anchorSemitone: semi,
			snapshot: snapMap
		};
		svgEl.setPointerCapture(e.pointerId);
		e.preventDefault();
	}

	function onPointerMove(e: PointerEvent) {
		if (!svgEl) return;
		const currentDrag = drag;
		if (!currentDrag) return;
		if (e.pointerId !== currentDrag.pointerId) return;

		const { x, y, width, height } = getLocalPoint(e, svgEl);
		const areaW = getAreaWidth(width);
		const localX = x - GUTTER_PX;
		const tRaw = xToTime(localX, areaW, track.duration);
		const t = snapTime(tRaw, bpm, snapEnabled);
		const semi = yToSemitone(y, height, MIN_SEMITONE, MAX_SEMITONE);

		if (currentDrag.mode === 'pen') {
			currentDrag.previewStart = Math.min(currentDrag.startTime, t);
			currentDrag.previewEnd = Math.max(currentDrag.startTime, t);
			return;
		}

		if (currentDrag.mode === 'move') {
			const anchor = snapTime(currentDrag.anchorTime, bpm, snapEnabled);
			const dt = t - anchor;
			const dSemi = semi - currentDrag.anchorSemitone;

			updateNotes((n) => {
				const base = currentDrag.snapshot.get(n.id);
				if (!base) return n;

				const dur = base.endTime - base.startTime;
				const newStart = clamp(base.startTime + dt, 0, Math.max(0, track.duration - dur));
				const newEnd = newStart + dur;
				const newBase = clamp(base.baseSemitone + dSemi, MIN_SEMITONE, MAX_SEMITONE);
				const newOffset = dSemi !== 0 ? n.pitchOffset + dSemi : n.pitchOffset;
				return { ...n, startTime: newStart, endTime: newEnd, baseSemitone: newBase, pitchOffset: newOffset };
			});
			return;
		}

		// resize
		const base = currentDrag.snapshot;
		const anchor = snapTime(currentDrag.anchorTime, bpm, snapEnabled);
		const dt = t - anchor;

		if (currentDrag.mode === 'resize-left') {
			const newStart = clamp(base.startTime + dt, 0, base.endTime - MIN_NOTE_SEC);
			updateNotes((n) => (n.id === currentDrag.activeId ? { ...n, startTime: newStart } : n));
			return;
		}

		const newEnd = clamp(base.endTime + dt, base.startTime + MIN_NOTE_SEC, track.duration);
		updateNotes((n) => (n.id === currentDrag.activeId ? { ...n, endTime: newEnd } : n));
	}

	function onPointerUp(e: PointerEvent) {
		if (!svgEl) return;
		if (!drag) return;
		if (e.pointerId !== drag.pointerId) return;

		if (drag.mode === 'pen') {
			const startTime = clamp(drag.previewStart, 0, track.duration);
			const endTime = clamp(drag.previewEnd, 0, track.duration);
			if (endTime > startTime + MIN_NOTE_SEC) {
				const seg = createNoteSegment({
					startTime,
					endTime,
					baseSemitone: drag.baseSemitone,
					pitchOffset: drag.baseSemitone - 60,
					enabled: true
				});
				onChange({
					...track,
					notes: [...track.notes, seg].sort((a, b) => a.startTime - b.startTime)
				});
				setSelected([seg.id]);
			}
		}

		try {
			svgEl.releasePointerCapture(e.pointerId);
		} catch {
			// ignore
		}
		drag = null;
	}
</script>

<div class="w:100% h:100% flex flex:column gap:10px">
	<div class="flex flex:row gap:10px ai:center">
		<button
			type="button"
			class={
				editorState.tool === 'select'
					? 'p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center'
					: 'p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center'
			}
			onclick={() => setTool('select')}
		>
			Select (1)
		</button>
		<button
			type="button"
			class={
				editorState.tool === 'pen'
					? 'p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center'
					: 'p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center'
			}
			onclick={() => setTool('pen')}
		>
			Pen (2)
		</button>
		<button
			type="button"
			class={
				editorState.tool === 'erase'
					? 'p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center'
					: 'p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center'
			}
			onclick={() => setTool('erase')}
		>
			Erase (3)
		</button>

		<label class="flex flex:row gap:8px ai:center">
			<input type="checkbox" bind:checked={snapEnabled} />
			snap
		</label>
		<label class="flex flex:row gap:8px ai:center">
			BPM
			<input
				class="w:90px p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
				type="number"
				min="1"
				step="1"
				bind:value={bpm}
			/>
		</label>
	</div>

	<div
		bind:this={editor}
		class="w:100% h:600px b:2px|solid|#222 r:6px"
		style="overflow-y:auto; overflow-x:hidden;"
	>
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<svg
			bind:this={svgEl}
			class="outline:none:focus"
			width={svgW}
			height={SVG_H}
			viewBox={`0 0 ${svgW} ${SVG_H}`}
			role="application"
			aria-label="ノートエディタ"
			onpointerdown={onPointerDown}
			onpointermove={onPointerMove}
			onpointerup={onPointerUp}
			onpointercancel={onPointerUp}
		>
			<!-- grid: vertical beat lines -->
			{#if beatSec() > 0}
				{#each Array(Math.floor(track.duration / beatSec()) + 1) as _, i (i)}
					{@const t = i * beatSec()}
					{@const x = GUTTER_PX + timeToX(t, areaW(), track.duration)}
					<line x1={x} y1={0} x2={x} y2={SVG_H} stroke="rgba(0,0,0,0.10)" />
				{/each}
			{/if}

			<!-- grid: horizontal semitone lines + left labels -->
			{#each Array(MAX_SEMITONE - MIN_SEMITONE + 1) as _, i (i)}
				{@const semi = MAX_SEMITONE - i}
				{@const y = i * ROW_H_PX}
				<line x1={0} y1={y} x2={svgW} y2={y} stroke="rgba(0,0,0,0.10)" />
				<text
					x={6}
					y={y + ROW_H_PX * 0.75}
					font-size="12"
					fill="rgba(0,0,0,0.65)"
					style="user-select:none"
				>
					{getNoteName(semi)}
				</text>
			{/each}

			<!-- gutter separator -->
			<line x1={GUTTER_PX} y1={0} x2={GUTTER_PX} y2={SVG_H} stroke="rgba(0,0,0,0.25)" />

			{#each track.notes as n (n.id)}
				{@const r = noteRect(n)}
				<rect
					x={r.x}
					y={r.y}
					width={r.w}
					height={r.h}
					opacity={n.enabled ? 1 : 0.3}
					fill={
						editorState.selectedNoteIds.includes(n.id)
							? 'rgba(0, 120, 255, 0.55)'
							: 'rgba(0,0,0,0.25)'
					}
					stroke="rgba(0,0,0,0.35)"
				/>
			{/each}

			{#if drag && drag.mode === 'pen'}
				{@const x0 = GUTTER_PX + timeToX(drag.previewStart, areaW(), track.duration)}
				{@const x1 = GUTTER_PX + timeToX(drag.previewEnd, areaW(), track.duration)}
				{@const h = Math.max(8, ROW_H_PX * 0.85)}
				{@const yTop = semitoneToYTop(drag.baseSemitone, SVG_H, MIN_SEMITONE, MAX_SEMITONE)}
				{@const y0 = yTop + (ROW_H_PX - h) / 2}
				<rect
					x={Math.min(x0, x1)}
					y={y0}
					width={Math.max(2, Math.abs(x1 - x0))}
					height={h}
					fill="rgba(0,0,0,0.15)"
					stroke="rgba(0,0,0,0.35)"
				/>
			{/if}
		</svg>
	</div>
</div>
