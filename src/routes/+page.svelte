<script lang="ts">
    import { browser } from '$app/environment';
    import { onDestroy, onMount } from 'svelte';
    import init, { MelodyEngine } from 'melody-dsp';
    import { createNoteSegment, type NoteSegment, type NoteTrack } from '$lib/note-model';
    import NoteEditor from '$lib/components/NoteEditor.svelte';
    import test from '$lib/assets/test.wav?url'; //TODO: dev only

    // AudioWorkletは "実ファイルのURL" を addModule() へ渡す必要がある。
    // `?url` を使うと「URL文字列をexportするだけのViteモジュール」になり、
    // registerProcessor が実行されず nodeName 未定義になる。
    const workletModuleUrl = new URL('../lib/audio/worklet-processor.ts', import.meta.url);
    import wasmUrl from 'melody-dsp/melody_dsp_bg.wasm?url';

    let semitones = 0;

    let ctx: AudioContext | null = null;
    let workletNode: AudioWorkletNode | null = null;
    let sourceNode: AudioBufferSourceNode | null = null;

    let loadedName = '';
    let loadedBuffer: AudioBuffer | null = null;
    let renderedBuffer: AudioBuffer | null = null;

    let noteTrack: NoteTrack | null = null;
    let selectedNoteIds: string[] = [];

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
        const offsets = new Float32Array(enabledNotes.map((n) => n.pitchOffset));
        engine.set_notes(starts, ends, offsets);

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
        >再生 / Worklet</button>
        <button
            class="p:6px|8px bg:#333 fg:white r:6px flex ai:center jc:center"
            onclick={playRendered}
            disabled={!renderedBuffer}
        >再生 / Render</button>
        <button
            class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
            onclick={stop}
        >停止</button>
    </div>

    <div class="flex flex:row gap:20px">
        <button
            class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
            onclick={() => {
                if (!loadedBuffer) return;
                if (!noteTrack) noteTrack = ensureNoteTrackFromBuffer(loadedBuffer);
                noteTrack = { ...noteTrack, notes: makePresetNotes(noteTrack.duration) };
                selectedNoteIds = noteTrack.notes[0]?.id ? [noteTrack.notes[0].id] : [];
            }}
            disabled={!loadedBuffer}
        >
            ノートプリセット適用
        </button>
        <button
            class="p:6px|8px b:2px|solid|#333 r:6px flex ai:center jc:center"
            onclick={renderWithNotes}
            disabled={!loadedBuffer || !noteTrack}
        >Render & Play用にレンダ</button>
    </div>

    {#if noteTrack}
        <NoteEditor
            track={noteTrack}
            selectedNoteIds={selectedNoteIds}
            onSelect={(ids) => (selectedNoteIds = ids)}
            onChange={(t) => (noteTrack = t)}
        />
    {/if}
</section>