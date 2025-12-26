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
		yToSemitoneFloat,
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
	let pitchSnapEnabled = $state(true);

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

	function getPitchValueFromY(y: number): number {
		return pitchSnapEnabled
			? yToSemitone(y, SVG_H, MIN_SEMITONE, MAX_SEMITONE)
			: yToSemitoneFloat(y, SVG_H, MIN_SEMITONE, MAX_SEMITONE);
	}

	function clampAmount01to2(v: number): number {
		if (!Number.isFinite(v)) return 1;
		return Math.max(0, Math.min(2, v));
	}

	function pitchCurveSemitoneDelta(n: NoteSegment, tSec: number): number {
		const t0 = n.startTime;
		const t1 = n.endTime;
		const dur = Math.max(1e-6, t1 - t0);
		const u = (tSec - t0) / dur;
		const center = n.pitchOffset + n.pitchCenterOffset;
		const modHz = 5.5;
		const modAmp = 0.25 * n.pitchModAmount;
		const driftAmp = 0.2 * n.pitchDriftAmount;
		const mod = Math.sin(2 * Math.PI * modHz * (tSec - t0)) * modAmp;
		const drift = (u - 0.5) * 2.0 * driftAmp;
		return center + mod + drift;
	}

	function notePitchCurvePoints(n: NoteSegment, samples = 24): string {
		const x0 = GUTTER_PX + timeToX(n.startTime, areaW(), track.duration);
		const x1 = GUTTER_PX + timeToX(n.endTime, areaW(), track.duration);
		const dur = Math.max(1e-6, n.endTime - n.startTime);

		const pts: string[] = [];
		const count = Math.max(2, Math.floor(samples));
		for (let i = 0; i < count; i++) {
			const u = i / (count - 1);
			const tSec = n.startTime + dur * u;
			const absSemi = n.baseSemitone + pitchCurveSemitoneDelta(n, tSec);
			const yTop = semitoneToYTop(absSemi, SVG_H, MIN_SEMITONE, MAX_SEMITONE);
			const y = yTop + ROW_H_PX / 2;
			const x = x0 + (x1 - x0) * u;
			pts.push(`${x.toFixed(2)},${y.toFixed(2)}`);
		}
		return pts.join(' ');
	}

	function formatSigned(v: number): string {
		if (!Number.isFinite(v) || Math.abs(v) < 1e-3) return '0';
		const sign = v > 0 ? '+' : '';
		const abs = Math.abs(v);
		// 半音単位編集が多いので 1桁小数まで
		const s = abs >= 10 ? abs.toFixed(0) : abs.toFixed(1).replace(/\.0$/, '');
		return `${sign}${s}`;
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
		}
		| {
			mode:
				| 'pitch-center'
				| 'pitch-mod'
				| 'pitch-drift'
				| 'formant'
				| 'time-stretch-start'
				| 'time-stretch-end';
			pointerId: number;
			activeId: string;
			anchorX: number;
			anchorY: number;
			snapshot: {
				pitchCenterOffset: number;
				pitchModAmount: number;
				pitchDriftAmount: number;
				formantShift: number;
				timeStretchStart: number;
				timeStretchEnd: number;
			};
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
			if (e.key === '4') {
				e.preventDefault();
				setTool('pitch-center-tool');
			}
			if (e.key === '5') {
				e.preventDefault();
				setTool('pitch-mod-tool');
			}
			if (e.key === '6') {
				e.preventDefault();
				setTool('pitch-drift-tool');
			}
			if (e.key === '7') {
				e.preventDefault();
				setTool('time-tool');
			}
			if (e.key === '8') {
				e.preventDefault();
				setTool('formant-tool');
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
		const semi = getPitchValueFromY(y);
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

		if (
			editorState.tool === 'pitch-center-tool' ||
			editorState.tool === 'pitch-mod-tool' ||
			editorState.tool === 'pitch-drift-tool' ||
			editorState.tool === 'formant-tool' ||
			editorState.tool === 'time-tool'
		) {
			if (!hit) {
				if (!multi) setSelected([]);
				return;
			}

			const active = hit.note;
			setSelected([active.id]);

			const baseDrag = {
				pointerId: e.pointerId,
				activeId: active.id,
				anchorX: x,
				anchorY: y,
				snapshot: {
					pitchCenterOffset: active.pitchCenterOffset,
					pitchModAmount: active.pitchModAmount,
					pitchDriftAmount: active.pitchDriftAmount,
					formantShift: active.formantShift,
					timeStretchStart: active.timeStretchStart,
					timeStretchEnd: active.timeStretchEnd
				}
			};

			if (editorState.tool === 'pitch-center-tool') drag = { mode: 'pitch-center', ...baseDrag };
			if (editorState.tool === 'pitch-mod-tool') drag = { mode: 'pitch-mod', ...baseDrag };
			if (editorState.tool === 'pitch-drift-tool') drag = { mode: 'pitch-drift', ...baseDrag };
			if (editorState.tool === 'formant-tool') drag = { mode: 'formant', ...baseDrag };
			if (editorState.tool === 'time-tool') {
				const r = noteRect(active);
				const chooseEnd = hit.part === 'body' ? x >= r.x + r.w / 2 : hit.part === 'right';
				drag = {
					mode: chooseEnd ? 'time-stretch-end' : 'time-stretch-start',
					...baseDrag
				};
			}

			svgEl.setPointerCapture(e.pointerId);
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
		const semi = getPitchValueFromY(y);

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
				return { ...n, startTime: newStart, endTime: newEnd, baseSemitone: newBase };
			});
			return;
		}

		if (
			currentDrag.mode === 'pitch-center' ||
			currentDrag.mode === 'formant' ||
			currentDrag.mode === 'pitch-mod' ||
			currentDrag.mode === 'pitch-drift'
		) {
			const dy = currentDrag.anchorY - y;
			const dSemi = pitchSnapEnabled ? Math.round(dy / ROW_H_PX) : dy / ROW_H_PX;

			if (currentDrag.mode === 'pitch-center') {
				updateNotes((n) =>
					n.id === currentDrag.activeId
						? { ...n, pitchCenterOffset: currentDrag.snapshot.pitchCenterOffset + dSemi }
						: n
				);
				return;
			}

			if (currentDrag.mode === 'formant') {
				updateNotes((n) =>
					n.id === currentDrag.activeId
						? { ...n, formantShift: currentDrag.snapshot.formantShift + dSemi }
						: n
				);
				return;
			}

			const dAmt = (currentDrag.anchorY - y) / 120;
			if (currentDrag.mode === 'pitch-mod') {
				const next = clampAmount01to2(currentDrag.snapshot.pitchModAmount + dAmt);
				updateNotes((n) => (n.id === currentDrag.activeId ? { ...n, pitchModAmount: next } : n));
				return;
			}

			const next = clampAmount01to2(currentDrag.snapshot.pitchDriftAmount + dAmt);
			updateNotes((n) => (n.id === currentDrag.activeId ? { ...n, pitchDriftAmount: next } : n));
			return;
		}

		if (currentDrag.mode === 'time-stretch-start' || currentDrag.mode === 'time-stretch-end') {
			const dx = x - currentDrag.anchorX;
			const scaleDelta = dx / 200;
			const clampStretch = (v: number) => Math.max(0.5, Math.min(2.0, v));
			if (currentDrag.mode === 'time-stretch-start') {
				const next = clampStretch(currentDrag.snapshot.timeStretchStart * (1 + scaleDelta));
				updateNotes((n) => (n.id === currentDrag.activeId ? { ...n, timeStretchStart: next } : n));
				return;
			}
			const next = clampStretch(currentDrag.snapshot.timeStretchEnd * (1 + scaleDelta));
			updateNotes((n) => (n.id === currentDrag.activeId ? { ...n, timeStretchEnd: next } : n));
			return;
		}

		if (currentDrag.mode === 'resize-left' || currentDrag.mode === 'resize-right') {
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
			return;
		}
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
			<input type="checkbox" bind:checked={pitchSnapEnabled} />
			pitch snap
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

		<button
			type="button"
			class={
				editorState.tool === 'pitch-center-tool'
					? 'p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center'
					: 'p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center'
			}
			onclick={() => setTool('pitch-center-tool')}
		>
			Center (4)
		</button>
		<button
			type="button"
			class={
				editorState.tool === 'pitch-mod-tool'
					? 'p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center'
					: 'p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center'
			}
			onclick={() => setTool('pitch-mod-tool')}
		>
			Mod (5)
		</button>
		<button
			type="button"
			class={
				editorState.tool === 'pitch-drift-tool'
					? 'p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center'
					: 'p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center'
			}
			onclick={() => setTool('pitch-drift-tool')}
		>
			Drift (6)
		</button>
		<button
			type="button"
			class={
				editorState.tool === 'time-tool'
					? 'p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center'
					: 'p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center'
			}
			onclick={() => setTool('time-tool')}
		>
			Time (7)
		</button>
		<button
			type="button"
			class={
				editorState.tool === 'formant-tool'
					? 'p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center'
					: 'p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center'
			}
			onclick={() => setTool('formant-tool')}
		>
			Formant (8)
		</button>
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
				{#if editorState.selectedNoteIds.includes(n.id)}
					<polyline
						points={notePitchCurvePoints(n)}
						fill="none"
						stroke="rgba(0,0,0,0.65)"
						stroke-width="1.5"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
					<text
						x={r.x + r.w - 4}
						y={r.y + r.h - 4}
						text-anchor="end"
						font-size="11"
						fill="rgba(0,0,0,0.65)"
						style="user-select:none"
					>
						F:{formatSigned(n.formantShift)}
					</text>
				{/if}
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
