<script lang="ts">
	import type { KeyScale } from '$lib/scale';
	import type { NoteSegment, NoteTrack } from '$lib/note-model';
	import { getSelectedOrAllNotes } from '$lib/note-model';
	import {
		applyCorrectPitchMacro,
		applyQuantizeTimeMacro,
		applyReduceDriftMacro,
		type TimeGridSettings
	} from '$lib/macros';

	type Props = {
		track: NoteTrack;
		selectedNoteIds: string[];
		keyScale: KeyScale;
		onChange: (next: NoteTrack) => void;
	};

	let { track, selectedNoteIds, keyScale, onChange }: Props = $props();

	let correctPitchAmount = $state(0);
	let reduceDriftAmount = $state(0);
	let quantizeStrength = $state(0);

	let grid = $state<TimeGridSettings>({ bpm: 120, division: 16 });

	type MacroName = 'correctPitch' | 'reduceDrift' | 'quantizeTime';
	let dragBaseNotes = $state<Record<MacroName, NoteSegment[] | null>>({
		correctPitch: null,
		reduceDrift: null,
		quantizeTime: null
	});
	let dragBaseSelectedIds = $state<Record<MacroName, string[]>>({
		correctPitch: [],
		reduceDrift: [],
		quantizeTime: []
	});

	function startDrag(name: MacroName) {
		dragBaseNotes = { ...dragBaseNotes, [name]: track.notes.map((n) => ({ ...n })) };
		dragBaseSelectedIds = { ...dragBaseSelectedIds, [name]: [...selectedNoteIds] };
	}

	function endDrag(name: MacroName) {
		dragBaseNotes = { ...dragBaseNotes, [name]: null };
	}

	function mergeUpdatedSubset(all: NoteSegment[], updated: NoteSegment[]): NoteSegment[] {
		const byId = new Map(updated.map((n) => [n.id, n] as const));
		return all.map((n) => byId.get(n.id) ?? n);
	}

	function applyMacro(name: MacroName, updatedSubset: NoteSegment[], baseNotes: NoteSegment[]) {
		const merged = mergeUpdatedSubset(baseNotes, updatedSubset).sort((a, b) => a.startTime - b.startTime);
		onChange({ ...track, notes: merged });
	}

	function onCorrectPitchInput(v: number) {
		correctPitchAmount = v;
		const baseNotes = dragBaseNotes.correctPitch ?? track.notes;
		const baseSel = dragBaseSelectedIds.correctPitch;
		const targets = getSelectedOrAllNotes(baseNotes, baseSel);
		applyMacro('correctPitch', applyCorrectPitchMacro(targets, v, keyScale), baseNotes);
	}

	function onReduceDriftInput(v: number) {
		reduceDriftAmount = v;
		const baseNotes = dragBaseNotes.reduceDrift ?? track.notes;
		const baseSel = dragBaseSelectedIds.reduceDrift;
		const targets = getSelectedOrAllNotes(baseNotes, baseSel);
		applyMacro('reduceDrift', applyReduceDriftMacro(targets, v), baseNotes);
	}

	function onQuantizeTimeInput(v: number) {
		quantizeStrength = v;
		const baseNotes = dragBaseNotes.quantizeTime ?? track.notes;
		const baseSel = dragBaseSelectedIds.quantizeTime;
		const targets = getSelectedOrAllNotes(baseNotes, baseSel);
		applyMacro('quantizeTime', applyQuantizeTimeMacro(targets, v, grid), baseNotes);
	}
</script>

<section class="flex flex:column gap:14px">
	<div class="opacity:0.85">対象: {selectedNoteIds.length > 0 ? '選択ノート' : '全ノート'}</div>

	<section class="flex flex:column gap:8px">
		<div class="opacity:0.9">Correct Pitch</div>
		<div class="flex flex:row gap:10px ai:center">
			<input
				type="range"
				min="0"
				max="100"
				step="1"
				value={correctPitchAmount}
				onpointerdown={() => startDrag('correctPitch')}
				onpointerup={() => endDrag('correctPitch')}
				oninput={(e) => onCorrectPitchInput(Number((e.currentTarget as HTMLInputElement).value))}
			/>
			<span>{correctPitchAmount.toFixed(0)}%</span>
		</div>
	</section>

	<section class="flex flex:column gap:8px">
		<div class="opacity:0.9">Reduce Drift</div>
		<div class="flex flex:row gap:10px ai:center">
			<input
				type="range"
				min="0"
				max="100"
				step="1"
				value={reduceDriftAmount}
				onpointerdown={() => startDrag('reduceDrift')}
				onpointerup={() => endDrag('reduceDrift')}
				oninput={(e) => onReduceDriftInput(Number((e.currentTarget as HTMLInputElement).value))}
			/>
			<span>{reduceDriftAmount.toFixed(0)}%</span>
		</div>
	</section>

	<section class="flex flex:column gap:8px">
		<div class="opacity:0.9">Quantize Time</div>
		<div class="flex flex:row gap:10px ai:center">
			<span>Tempo</span>
			<input
				type="number"
				min="20"
				max="300"
				step="1"
				value={grid.bpm}
				oninput={(e) => {
					grid = { ...grid, bpm: Number((e.currentTarget as HTMLInputElement).value) };
				}}
				style="width:80px;"
			/>

			<span>Grid</span>
			<select
				value={String(grid.division)}
				onchange={(e) => {
					const v = Number((e.currentTarget as HTMLSelectElement).value) as 4 | 8 | 16;
					grid = { ...grid, division: v };
				}}
			>
				<option value="4">1/4</option>
				<option value="8">1/8</option>
				<option value="16">1/16</option>
			</select>
		</div>

		<div class="flex flex:row gap:10px ai:center">
			<input
				type="range"
				min="0"
				max="100"
				step="1"
				value={quantizeStrength}
				onpointerdown={() => startDrag('quantizeTime')}
				onpointerup={() => endDrag('quantizeTime')}
				oninput={(e) => onQuantizeTimeInput(Number((e.currentTarget as HTMLInputElement).value))}
			/>
			<span>{quantizeStrength.toFixed(0)}%</span>
		</div>
	</section>
</section>
