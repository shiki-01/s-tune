<script lang="ts">
	import { DEFAULT_HARMONICS_CONFIG } from '$lib/sound-model';

	export type SoundEditorMode = 'track' | 'selected-note';

	type Props = {
		mode: SoundEditorMode;
		trackHarmonics: number[];
		trackFormantShift: number;
		selectedNoteId: string | null;
		selectedNoteHarmonics: number[] | null;
		onChangeMode: (mode: SoundEditorMode) => void;
		onChangeTrackHarmonics: (harmonics: number[]) => void;
		onChangeTrackFormantShift: (formantShift: number) => void;
		onChangeSelectedNoteHarmonics: (noteId: string, harmonics: number[]) => void;
	};

	let {
		mode,
		trackHarmonics,
		trackFormantShift,
		selectedNoteId,
		selectedNoteHarmonics,
		onChangeMode,
		onChangeTrackHarmonics,
		onChangeTrackFormantShift,
		onChangeSelectedNoteHarmonics,
	}: Props = $props();

	const MIN_GAIN = DEFAULT_HARMONICS_CONFIG.minGain;
	const MAX_GAIN = DEFAULT_HARMONICS_CONFIG.maxGain;
	const BAR_AREA_H = 160;
	const MIN_DB = 20 * Math.log10(MIN_GAIN);
	const MAX_DB = 20 * Math.log10(MAX_GAIN);

	let barsEl: HTMLDivElement | null = $state(null);
	let activeDragIndex = $state<number | null>(null);
	let activeDragGain = $state<number | null>(null);

	function neutralHarmonics(len: number): number[] {
		return Array.from({ length: Math.max(0, len) }, () => 1.0);
	}

	let bars = $derived(() => {
		if (mode === 'track') return trackHarmonics;
		// per-note harmonics are relative. if missing, default to 1.0 (0dB)
		return selectedNoteHarmonics ?? neutralHarmonics(trackHarmonics.length);
	});

	function clamp(v: number, lo: number, hi: number): number {
		if (!Number.isFinite(v)) return lo;
		return Math.max(lo, Math.min(hi, v));
	}

	function gainToDb(g: number): number {
		if (!Number.isFinite(g) || g <= 0) return Number.NEGATIVE_INFINITY;
		return 20 * Math.log10(g);
	}

	function dbToGain(db: number): number {
		if (!Number.isFinite(db)) return 1;
		return Math.pow(10, db / 20);
	}

	function gainFromY(y: number, rect: DOMRect): number {
		const u = clamp(1 - (y - rect.top) / rect.height, 0, 1);
		const db = MIN_DB + (MAX_DB - MIN_DB) * u;
		return dbToGain(db);
	}

	function setHarmonicAt(index: number, gain: number) {
		const g = clamp(gain, MIN_GAIN, MAX_GAIN);
		activeDragIndex = index;
		activeDragGain = g;

		if (mode === 'track') {
			const next = [...trackHarmonics];
			next[index] = g;
			onChangeTrackHarmonics(next);
			return;
		}

		if (!selectedNoteId) return;
		const base = selectedNoteHarmonics ?? neutralHarmonics(trackHarmonics.length);
		const next = [...base];
		next[index] = g;
		onChangeSelectedNoteHarmonics(selectedNoteId, next);
	}

	function onBarPointerDown(e: PointerEvent, index: number) {
		if (!barsEl) return;
		if (mode === 'selected-note' && !selectedNoteId) return;
		(barsEl as HTMLElement).setPointerCapture(e.pointerId);
		const rect = barsEl.getBoundingClientRect();
		setHarmonicAt(index, gainFromY(e.clientY, rect));

		const onMove = (ev: PointerEvent) => {
			setHarmonicAt(index, gainFromY(ev.clientY, rect));
		};
		const onUp = (ev: PointerEvent) => {
			try {
				(barsEl as HTMLElement).releasePointerCapture(ev.pointerId);
			} catch {
				// ignore
			}
			activeDragIndex = null;
			activeDragGain = null;
			window.removeEventListener('pointermove', onMove);
			window.removeEventListener('pointerup', onUp);
		};

		window.addEventListener('pointermove', onMove);
		window.addEventListener('pointerup', onUp);
	}
</script>

<section class="flex flex:column gap:12px">
	<div class="flex flex:row gap:16px ai:center">
		<label class="flex flex:row gap:8px ai:center">
			<input
				type="radio"
				name="sound-editor-mode"
				checked={mode === 'track'}
				onchange={() => onChangeMode('track')}
			/>
			<span>全ノート共通</span>
		</label>
		<label class="flex flex:row gap:8px ai:center">
			<input
				type="radio"
				name="sound-editor-mode"
				checked={mode === 'selected-note'}
				onchange={() => onChangeMode('selected-note')}
			/>
			<span>選択ノートのみ</span>
		</label>

		{#if mode === 'selected-note'}
			<span class="opacity:0.8">
				{selectedNoteId ? `note: ${selectedNoteId.slice(0, 6)}` : 'ノート未選択'}
			</span>
		{/if}
	</div>

	{#if activeDragIndex != null && activeDragGain != null}
		<div class="opacity:0.85">
			H{activeDragIndex + 1}: {gainToDb(activeDragGain).toFixed(1)} dB（{activeDragGain.toFixed(2)}x）
		</div>
	{/if}

	<div class="flex flex:row gap:10px ai:center">
		<span>Formant</span>
		<input
			type="range"
			min="-6"
			max="6"
			step="0.1"
			value={trackFormantShift}
			oninput={(e) => {
				onChangeTrackFormantShift(Number((e.currentTarget as HTMLInputElement).value));
			}}
		/>
		<span>{trackFormantShift.toFixed(1)} st</span>
	</div>

	<div
		bind:this={barsEl}
		class="flex flex:row gap:6px ai:end"
		style={`height:${BAR_AREA_H}px; user-select:none; touch-action:none;`}
	>
		{#each bars() as g, idx}
			<div class="flex flex:column ai:center gap:4px" style="width:18px;">
				<div
					class="bg:#333 r:4px"
					style={`width:18px; height:${(() => {
							const db = gainToDb(g);
							const u = clamp((db - MIN_DB) / (MAX_DB - MIN_DB), 0, 1);
							return Math.round(u * BAR_AREA_H);
						})()}px; ${mode === 'selected-note' && !selectedNoteId ? 'opacity:0.35;' : ''}`}
					onpointerdown={(e) => onBarPointerDown(e, idx)}
				></div>
				<div class="opacity:0.7" style="font-size:10px;">{idx + 1}</div>
			</div>
		{/each}
	</div>

	<div class="opacity:0.75">
		範囲: {MIN_DB.toFixed(0)}〜{MAX_DB.toFixed(0)} dB（1.0が0dB）
	</div>
</section>
