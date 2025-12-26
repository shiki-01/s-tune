<script lang="ts">
    import { browser } from '$app/environment';
    import { onDestroy, onMount } from 'svelte';
    import init, { MelodyEngine } from 'melody-dsp';
    import { createNoteSegment, type NoteSegment, type NoteTrack } from '$lib/note-model';
    import NoteEditor from '$lib/components/NoteEditor.svelte';
    import SoundEditor from '$lib/components/SoundEditor.svelte';
    import type { SoundEditorMode } from '$lib/components/SoundEditor.svelte';
    import { createDummyPitchDetector } from '$lib/pitch-detection';
    import { detectNotesFromPitch, detectedNotesToNoteSegments, makeKey, snapMidiToScale, type ScaleName } from '$lib/note-detection';
    import { createDefaultHarmonics, DEFAULT_HARMONICS_CONFIG, type HarmonicProfile, type TrackMeanSpectrum } from '$lib/sound-model';
    import test from '$lib/assets/test.wav?url'; //TODO: dev only

    // AudioWorkletは "実ファイルのURL" を addModule() へ渡す必要がある。
    // `?url` を使うと「URL文字列をexportするだけのViteモジュール」になり、
    // registerProcessor が実行されず nodeName 未定義になる。
    const workletModuleUrl = new URL('../lib/audio/worklet-processor.ts', import.meta.url);
    import wasmUrl from 'melody-dsp/melody_dsp_bg.wasm?url';

    let semitones = $state(0);

    let ctx: AudioContext | null = $state(null);
    let workletNode: AudioWorkletNode | null = $state(null);
    let sourceNode: AudioBufferSourceNode | null = $state(null);

    let loadedName = $state('');
    let loadedBuffer: AudioBuffer | null = $state(null);
    let renderedBuffer: AudioBuffer | null = $state(null);

    let noteTrack: NoteTrack | null = $state(null);
    let selectedNoteIds: string[] = $state([]);

    let activePanel = $state<'notes' | 'sound'>('notes');

    let soundEditorMode = $state<SoundEditorMode>('track');
    let trackMeanSpectrum = $state<TrackMeanSpectrum>({
        harmonics: createDefaultHarmonics(DEFAULT_HARMONICS_CONFIG)
    });
    let noteHarmonicProfiles = $state<Record<string, HarmonicProfile>>({});

    function getSelectedNote() {
        const id = selectedNoteIds[0];
        if (!noteTrack || !id) return null;
        return noteTrack.notes.find((n) => n.id === id) ?? null;
    }

    function getSelectedNoteHarmonics(): number[] | null {
        const n = getSelectedNote();
        if (!n) return null;
        return noteHarmonicProfiles[n.id]?.harmonics ?? null;
    }

    function upsertNoteHarmonics(noteId: string, harmonics: number[]) {
        noteHarmonicProfiles = {
            ...noteHarmonicProfiles,
            [noteId]: { noteId, harmonics }
        };
        renderedBuffer = null;
    }

    function updateSelectedNoteFormantShift(noteId: string, formantShift: number) {
        if (!noteTrack) return;
        noteTrack = {
            ...noteTrack,
            notes: noteTrack.notes.map((n) => (n.id === noteId ? { ...n, formantShift } : n))
        };
        renderedBuffer = null;
    }

    const ROOT_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'] as const;
    let keyRootPc = $state(0);
    let scaleName = $state<ScaleName>('major');
    let key = $derived(() => makeKey(keyRootPc, scaleName, 60));

    let lastAutoDetectedNoteCount = $state<number | null>(null);

    function resnapExistingNotes() {
        if (!noteTrack) return;
        noteTrack = {
            ...noteTrack,
            notes: noteTrack.notes.map((n) => ({ ...n, snappedSemitone: snapMidiToScale(n.baseSemitone, key()) }))
        };
    }

    async function autoDetectNotes() {
        if (!loadedBuffer) return;
        if (!noteTrack) noteTrack = ensureNoteTrackFromBuffer(loadedBuffer);

        const detector = createDummyPitchDetector();
        const frames = await detector.detectPitch(loadedBuffer);
        const detected = detectNotesFromPitch(frames);
        const nextNotes = detectedNotesToNoteSegments(detected, key());

        noteTrack = { ...noteTrack, notes: nextNotes };
        selectedNoteIds = nextNotes[0]?.id ? [nextNotes[0].id] : [];
        lastAutoDetectedNoteCount = nextNotes.length;
        renderedBuffer = null;
    }

    async function ensureAudioGraph() {
        if (!browser) return;
        if (ctx && workletNode) return;

        ctx = new AudioContext();

        await ctx.audioWorklet.addModule(workletModuleUrl);

        // wasm-bindgen の init() が Worklet 内で URL を使って落ちる環境があるため、
        // メインスレッド側で .wasm を取って ArrayBuffer を渡す。
        const wasmBytes = await fetch(wasmUrl).then((r) => r.arrayBuffer());

        workletNode = new AudioWorkletNode(ctx, 'melody-processor', {
            numberOfInputs: 1,
            numberOfOutputs: 1,
            outputChannelCount: [1],
            processorOptions: { sampleRate: ctx.sampleRate, wasmBytes }
        });

        // 初期値送信
        workletNode.port.postMessage({ type: 'semitones', value: semitones });

        // Worklet -> 出力
        workletNode.connect(ctx.destination);
    }

    function ensureNoteTrackFromBuffer(buf: AudioBuffer): NoteTrack {
        return {
            sampleRate: buf.sampleRate,
            duration: buf.duration,
            notes: []
        };
    }

    function makePresetNotes(duration: number): NoteSegment[] {
        // 例：前半を +3、後半を -3（ファイルが長ければ先頭2秒/末尾2秒に寄せる）
        const base = 60;
        if (duration >= 4) {
            return [
                createNoteSegment({ startTime: 0, endTime: 2, baseSemitone: base, pitchOffset: 3, enabled: true }),
                createNoteSegment({ startTime: Math.max(0, duration - 2), endTime: duration, baseSemitone: base, pitchOffset: -3, enabled: true })
            ];
        }
        const mid = duration * 0.5;
        return [
            createNoteSegment({ startTime: 0, endTime: mid, baseSemitone: base, pitchOffset: 3, enabled: true }),
            createNoteSegment({ startTime: mid, endTime: duration, baseSemitone: base, pitchOffset: -3, enabled: true })
        ];
    }

    function selectNoteByTime(timeSec: number) {
        if (!noteTrack) return;
        const hit = noteTrack.notes.find((n) => timeSec >= n.startTime && timeSec < n.endTime);
        selectedNoteIds = hit?.id ? [hit.id] : [];
    }

    function updateSelectedPitchOffset(v: number) {
        const selectedNoteId = selectedNoteIds[0];
        if (!noteTrack || !selectedNoteId) return;
        noteTrack = {
            ...noteTrack,
            notes: noteTrack.notes.map((n) => (n.id === selectedNoteId ? { ...n, pitchOffset: v } : n))
        };
    }

    function updateSelectedEnabled(checked: boolean) {
        const selectedNoteId = selectedNoteIds[0];
        if (!noteTrack || !selectedNoteId) return;
        noteTrack = {
            ...noteTrack,
            notes: noteTrack.notes.map((n) => (n.id === selectedNoteId ? { ...n, enabled: checked } : n))
        };
    }

    async function renderWithNotes() {
        if (!loadedBuffer) return;
        await ensureAudioGraph();
        if (!ctx) return;

        if (!noteTrack) {
            noteTrack = ensureNoteTrackFromBuffer(loadedBuffer);
        }

        // wasm init（メインスレッドなのでURL系の問題なし）
        await init();

        const engine = new MelodyEngine(noteTrack.sampleRate);
        const enabledNotes = noteTrack.notes.filter((n) => n.enabled && n.endTime > n.startTime);
        const starts = new Float32Array(enabledNotes.map((n) => n.startTime));
        const ends = new Float32Array(enabledNotes.map((n) => n.endTime));
        const baseSemitones = new Float32Array(enabledNotes.map((n) => n.baseSemitone));
        const offsets = new Float32Array(enabledNotes.map((n) => n.pitchOffset));
        const centerOffsets = new Float32Array(enabledNotes.map((n) => n.pitchCenterOffset));
        const modAmounts = new Float32Array(enabledNotes.map((n) => n.pitchModAmount));
        const driftAmounts = new Float32Array(enabledNotes.map((n) => n.pitchDriftAmount));
        const timeStretchStarts = new Float32Array(enabledNotes.map((n) => n.timeStretchStart));
        const timeStretchEnds = new Float32Array(enabledNotes.map((n) => n.timeStretchEnd));
        const formantShifts = new Float32Array(enabledNotes.map((n) => n.formantShift));

        const harmonicsPerNote = Math.max(1, Math.floor(trackMeanSpectrum.harmonics.length));
        const trackGains = new Float32Array(trackMeanSpectrum.harmonics);
        const noteHarmonicsFlat = new Float32Array(enabledNotes.length * harmonicsPerNote);
        for (let i = 0; i < enabledNotes.length; i++) {
            const noteId = enabledNotes[i]!.id;
            const prof = noteHarmonicProfiles[noteId]?.harmonics;
            for (let h = 0; h < harmonicsPerNote; h++) {
                // per-note profile is relative (1.0 = 0dB). if missing, default to 1.0
                noteHarmonicsFlat[i * harmonicsPerNote + h] = prof?.[h] ?? 1.0;
            }
        }

        const anyEngine = engine as unknown as {
            set_harmonic_gains?: (gains: Float32Array) => void;
            set_notes: (...args: unknown[]) => void;
        };

        // New DSP features are available only after rebuilding melody-dsp/pkg.
        if (typeof anyEngine.set_harmonic_gains === 'function') {
            anyEngine.set_harmonic_gains(trackGains);
        }

        // Prefer new signature when available; otherwise, fall back.
        const declaredArgs = (anyEngine.set_notes as unknown as Function).length;
        if (declaredArgs >= 11) {
            anyEngine.set_notes(
                starts,
                ends,
                baseSemitones,
                offsets,
                centerOffsets,
                modAmounts,
                driftAmounts,
                timeStretchStarts,
                timeStretchEnds,
                formantShifts,
                harmonicsPerNote,
                noteHarmonicsFlat
            );
        } else {
            anyEngine.set_notes(
                starts,
                ends,
                offsets,
                centerOffsets,
                modAmounts,
                driftAmounts,
                timeStretchStarts,
                timeStretchEnds,
                formantShifts
            );
        }

        const input = loadedBuffer.getChannelData(0);
        const buf = new Float32Array(input); // 元を破壊しない
        engine.process_buffer(buf);

        renderedBuffer = new AudioBuffer({ length: buf.length, numberOfChannels: 1, sampleRate: loadedBuffer.sampleRate });
        renderedBuffer.copyToChannel(buf, 0);
    }

    async function playRendered() {
        await ensureAudioGraph();
        if (!ctx || !renderedBuffer) return;
        if (ctx.state !== 'running') {
            await ctx.resume();
        }
        stop();
        sourceNode = new AudioBufferSourceNode(ctx, { buffer: renderedBuffer });
        sourceNode.connect(ctx.destination);
        sourceNode.start();
        sourceNode.onended = () => {
            sourceNode?.disconnect();
            sourceNode = null;
        };
    }

    function downmixToMono(buf: AudioBuffer): AudioBuffer {
        if (!ctx) throw new Error('AudioContext not ready');

        if (buf.numberOfChannels === 1) return buf;

        const len = buf.length;
        const mono = new Float32Array(len);

        for (let ch = 0; ch < buf.numberOfChannels; ch++) {
            const data = buf.getChannelData(ch);
            for (let i = 0; i < len; i++) mono[i] += data[i];
        }
        for (let i = 0; i < len; i++) mono[i] /= buf.numberOfChannels;

        const out = new AudioBuffer({
            length: len,
            numberOfChannels: 1,
            sampleRate: buf.sampleRate
        });
        out.copyToChannel(mono, 0);
        return out;
    }

    async function onPickFile(e: Event) {
        const input = e.currentTarget as HTMLInputElement;
        const file = input.files?.[0];
        if (!file) return;

        await ensureAudioGraph();
        if (!ctx) return;

        loadedName = file.name;

        const ab = await file.arrayBuffer();
        const decoded = await ctx.decodeAudioData(ab.slice(0));
        loadedBuffer = downmixToMono(decoded);
        renderedBuffer = null;
        noteTrack = ensureNoteTrackFromBuffer(loadedBuffer);
        selectedNoteIds = [];
        lastAutoDetectedNoteCount = null;
    }

    async function play() {
        await ensureAudioGraph();
        if (!ctx || !workletNode || !loadedBuffer) return;

        if (ctx.state !== 'running') {
            await ctx.resume();
        }

        // 多重再生を防ぐ
        stop();

        sourceNode = new AudioBufferSourceNode(ctx, { buffer: loadedBuffer });
        // source -> Worklet
        sourceNode.connect(workletNode);
        sourceNode.start();
        sourceNode.onended = () => {
            sourceNode?.disconnect();
            sourceNode = null;
        };
    }

    function stop() {
        try {
            sourceNode?.stop();
        } catch {
            // already stopped
        }
        sourceNode?.disconnect();
        sourceNode = null;
    }

    function onSemitonesInput(v: number) {
        semitones = v;
        workletNode?.port.postMessage({ type: 'semitones', value: semitones });
    }

    onDestroy(() => {
        stop();
        workletNode?.disconnect();
        workletNode = null;
        void ctx?.close();
        ctx = null;
    });

    onMount(async () => {
        if (!browser) return;
        // dev だけ自動読み込みにしたいならこの if を付ける
        if (!import.meta.env.DEV) return;

        // すでに手動で読み込んでいる場合は何もしない
        if (loadedBuffer) return;

        await ensureAudioGraph();
        if (!ctx) return;

        try {
            const res = await fetch(test);
            const ab = await res.arrayBuffer();
            const decoded = await ctx.decodeAudioData(ab.slice(0));
            loadedBuffer = downmixToMono(decoded);
            renderedBuffer = null;
            noteTrack = ensureNoteTrackFromBuffer(loadedBuffer);
            loadedName = 'text.wav';
            selectedNoteIds = [];
        } catch (e) {
            console.error('Default audio load failed', e);
        }
    });
</script>

<section class="flex flex:column gap:20px w:100% h:100dvh p:20px">
    <label class="flex flex:row gap:10px ai:center jc:start">
        <span>音声ファイルを読み込む</span>
        <input type="file" accept="audio/*" onchange={onPickFile} />
    </label>

    <div>Loaded: {loadedName || '(none)'}</div>

    <div class="flex flex:row gap:10px">
        <button
            class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
            onclick={() => (activePanel = 'notes')}
        >Note Editor</button>
        <button
            class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
            onclick={() => (activePanel = 'sound')}
        >Sound Editor</button>
    </div>

    <label class="flex flex:row gap:10px">
        ピッチシフト（半音）: {semitones.toFixed(1)}
        <input
            type="range"
            min="-12"
            max="12"
            step="0.1"
            value={semitones}
            oninput={(e) => onSemitonesInput(Number((e.currentTarget as HTMLInputElement).value))}
        />
    </label>

    <div class="flex flex:row gap:20px">
        <button
            class="p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center"
            onclick={play}
            disabled={!loadedBuffer}
        >再生 / Worklet（ノート無視）</button>
        <button
            class="p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center"
            onclick={playRendered}
            disabled={!renderedBuffer}
        >再生 / Render（ノート反映）</button>
        <button
            class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
            onclick={stop}
        >停止</button>
    </div>

    <div class="flex flex:row gap:14px ai:center">
        <label class="flex flex:row gap:8px ai:center">
            <span>Key</span>
            <select
                value={String(keyRootPc)}
                onchange={(e) => {
                    keyRootPc = Number((e.currentTarget as HTMLSelectElement).value);
                    resnapExistingNotes();
                }}
            >
                {#each ROOT_NAMES as name, idx}
                    <option value={String(idx)}>{name}</option>
                {/each}
            </select>
        </label>

        <label class="flex flex:row gap:8px ai:center">
            <span>Scale</span>
            <select
                value={scaleName}
                onchange={(e) => {
                    scaleName = (e.currentTarget as HTMLSelectElement).value as ScaleName;
                    resnapExistingNotes();
                }}
            >
                <option value="major">Major</option>
                <option value="minor">Minor</option>
            </select>
        </label>

        {#if lastAutoDetectedNoteCount != null}
            <span>Auto: {lastAutoDetectedNoteCount} notes</span>
        {/if}
    </div>

    <div class="flex flex:row gap:20px">
        <button
            class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
            onclick={() => {
                if (!loadedBuffer) return;
                if (!noteTrack) noteTrack = ensureNoteTrackFromBuffer(loadedBuffer);
                noteTrack = { ...noteTrack, notes: makePresetNotes(noteTrack.duration) };
                selectedNoteIds = noteTrack.notes[0]?.id ? [noteTrack.notes[0].id] : [];
                lastAutoDetectedNoteCount = null;
            }}
            disabled={!loadedBuffer}
        >
            ノートプリセット適用
        </button>

        <button
            class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
            onclick={autoDetectNotes}
            disabled={!loadedBuffer}
        >Auto Detect Notes</button>

        <button
            class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
            onclick={renderWithNotes}
            disabled={!loadedBuffer || !noteTrack}
        >Render & Play用にレンダ</button>
    </div>

    {#if noteTrack && activePanel === 'notes'}
        <NoteEditor
            track={noteTrack}
            selectedNoteIds={selectedNoteIds}
            onSelect={(ids) => (selectedNoteIds = ids)}
            onChange={(t) => {
                noteTrack = t;
                renderedBuffer = null;
            }}
        />
    {/if}

    {#if noteTrack && activePanel === 'sound'}
        <SoundEditor
            mode={soundEditorMode}
            trackHarmonics={trackMeanSpectrum.harmonics}
            selectedNoteId={selectedNoteIds[0] ?? null}
            selectedNoteHarmonics={getSelectedNoteHarmonics()}
            selectedNoteFormantShift={getSelectedNote()?.formantShift ?? null}
            onChangeMode={(m) => (soundEditorMode = m)}
            onChangeTrackHarmonics={(h) => {
                trackMeanSpectrum = { ...trackMeanSpectrum, harmonics: h };
                renderedBuffer = null;
            }}
            onChangeSelectedNoteHarmonics={(noteId, h) => upsertNoteHarmonics(noteId, h)}
            onChangeSelectedNoteFormantShift={(noteId, v) => updateSelectedNoteFormantShift(noteId, v)}
        />
    {/if}
</section>