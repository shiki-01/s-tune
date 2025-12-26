<script lang="ts">
	import { DEFAULT_HARMONICS_CONFIG } from '$lib/sound-model';

	export type SoundEditorMode = 'track' | 'selected-note';

	type Props = {
		mode: SoundEditorMode;
		trackHarmonics: number[];
		selectedNoteId: string | null;
		selectedNoteHarmonics: number[] | null;
		selectedNoteFormantShift: number | null;
		onChangeMode: (mode: SoundEditorMode) => void;
		onChangeTrackHarmonics: (harmonics: number[]) => void;
		onChangeSelectedNoteHarmonics: (noteId: string, harmonics: number[]) => void;
		onChangeSelectedNoteFormantShift: (noteId: string, formantShift: number) => void;
	};

	let {
		mode,
		trackHarmonics,
		selectedNoteId,
		selectedNoteHarmonics,
		selectedNoteFormantShift,
		onChangeMode,
		onChangeTrackHarmonics,
		onChangeSelectedNoteHarmonics,
		onChangeSelectedNoteFormantShift
	}: Props = $props();

	const MIN_GAIN = DEFAULT_HARMONICS_CONFIG.minGain;
	const MAX_GAIN = DEFAULT_HARMONICS_CONFIG.maxGain;
	const BAR_AREA_H = 160;

	let barsEl: HTMLDivElement | null = $state(null);

	let activeHarmonics = $derived(() => {
		if (mode === 'selected-note') return selectedNoteHarmonics;
		return trackHarmonics;
	});

	let bars = $derived(() => (activeHarmonics() ?? trackHarmonics) as number[]);

	function clamp(v: number, lo: number, hi: number): number {
		if (!Number.isFinite(v)) return lo;
		return Math.max(lo, Math.min(hi, v));
	}

	function gainFromY(y: number, rect: DOMRect): number {
		const u = clamp(1 - (y - rect.top) / rect.height, 0, 1);
		return MIN_GAIN + (MAX_GAIN - MIN_GAIN) * u;
	}

	function setHarmonicAt(index: number, gain: number) {
		const g = clamp(gain, MIN_GAIN, MAX_GAIN);

		if (mode === 'track') {
			const next = [...trackHarmonics];
			next[index] = g;
			onChangeTrackHarmonics(next);
			return;
		}

		if (!selectedNoteId) return;
		const base = selectedNoteHarmonics ?? trackHarmonics;
		const next = [...base];
		next[index] = g;
		onChangeSelectedNoteHarmonics(selectedNoteId, next);
	}

	function onBarPointerDown(e: PointerEvent, index: number) {
		if (!barsEl) return;
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

	{#if mode === 'selected-note'}
		<div class="flex flex:row gap:10px ai:center">
			<span>Formant</span>
			<input
				type="range"
				min="-12"
				max="12"
				step="0.1"
				value={selectedNoteFormantShift ?? 0}
				disabled={!selectedNoteId}
				oninput={(e) => {
					if (!selectedNoteId) return;
					onChangeSelectedNoteFormantShift(
						selectedNoteId,
						Number((e.currentTarget as HTMLInputElement).value)
					);
				}}
			/>
			<span>{(selectedNoteFormantShift ?? 0).toFixed(1)} st</span>
		</div>
	{/if}

	<div
		bind:this={barsEl}
		class="flex flex:row gap:6px ai:end"
		style={`height:${BAR_AREA_H}px; user-select:none; touch-action:none;`}
	>
		{#each bars() as g, idx}
			<div class="flex flex:column ai:center gap:4px" style="width:18px;">
				<div
					class="bg:#333 r:4px"
					style={`width:18px; height:${
						Math.round(((g - MIN_GAIN) / (MAX_GAIN - MIN_GAIN)) * BAR_AREA_H)
					}px;`}
					onpointerdown={(e) => onBarPointerDown(e, idx)}
				></div>
				<div class="opacity:0.7" style="font-size:10px;">{idx + 1}</div>
			</div>
		{/each}
	</div>

	<div class="opacity:0.75">範囲: {MIN_GAIN.toFixed(2)}〜{MAX_GAIN.toFixed(2)}（1.0が基準）</div>
</section>
