<script lang="ts">
    import { browser } from '$app/environment';
    import { onDestroy } from 'svelte';
    import init, { MelodyEngine } from 'melody-dsp';
    import { createNoteSegment, type NoteSegment, type NoteTrack } from '$lib/note-types';
    import { encodeWavMono16 } from '$lib/audio/wav';

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
    let renderedWavUrl: string | null = null;

    let noteTrack: NoteTrack | null = null;
    let selectedNoteId: string | null = null;

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
        selectedNoteId = hit?.id ?? null;
    }

    function updateSelectedPitchOffset(v: number) {
        if (!noteTrack || !selectedNoteId) return;
        noteTrack = {
            ...noteTrack,
            notes: noteTrack.notes.map((n) => (n.id === selectedNoteId ? { ...n, pitchOffset: v } : n))
        };
    }

    function updateSelectedEnabled(checked: boolean) {
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

        // WAVダウンロード用
        if (renderedWavUrl) {
            URL.revokeObjectURL(renderedWavUrl);
            renderedWavUrl = null;
        }
        const wav = encodeWavMono16(buf, loadedBuffer.sampleRate);
        renderedWavUrl = URL.createObjectURL(wav);
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
        if (renderedWavUrl) {
            URL.revokeObjectURL(renderedWavUrl);
            renderedWavUrl = null;
        }
        noteTrack = ensureNoteTrackFromBuffer(loadedBuffer);
        noteTrack = { ...noteTrack, notes: makePresetNotes(noteTrack.duration) };
        selectedNoteId = noteTrack.notes[0]?.id ?? null;
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
        if (renderedWavUrl) {
            URL.revokeObjectURL(renderedWavUrl);
            renderedWavUrl = null;
        }
    });
</script>

<h1>s-tune (Melodyneライト 土台)</h1>

<section style="display: grid; gap: 12px; max-width: 720px;">
    <label>
        音声ファイルを読み込む（wav推奨 / モノラル化して再生）
        <input type="file" accept="audio/*" on:change={onPickFile} />
    </label>

    <div>Loaded: {loadedName || '(none)'}</div>

    <label>
        ピッチシフト（半音）: {semitones.toFixed(1)}
        <input
            type="range"
            min="-12"
            max="12"
            step="0.1"
            value={semitones}
            on:input={(e) => onSemitonesInput(Number((e.currentTarget as HTMLInputElement).value))}
        />
    </label>

    <div style="display: flex; gap: 8px;">
        <button on:click={play} disabled={!loadedBuffer}>再生（Workletプレビュー）</button>
        <button on:click={playRendered} disabled={!renderedBuffer}>再生（レンダ結果）</button>
        <button on:click={stop}>停止</button>
    </div>

    <div style="display: flex; gap: 8px; align-items: center; flex-wrap: wrap;">
        <button
            on:click={() => {
                if (!loadedBuffer) return;
                if (!noteTrack) noteTrack = ensureNoteTrackFromBuffer(loadedBuffer);
                noteTrack = { ...noteTrack, notes: makePresetNotes(noteTrack.duration) };
                selectedNoteId = noteTrack.notes[0]?.id ?? null;
            }}
            disabled={!loadedBuffer}
        >
            ノートプリセット適用
        </button>
        <button on:click={renderWithNotes} disabled={!loadedBuffer || !noteTrack}>ノートでレンダリング</button>
        {#if renderedWavUrl}
            <a href={renderedWavUrl} download="rendered.wav">WAVをダウンロード</a>
        {/if}
    </div>

    {#if noteTrack}
        <h2 style="margin: 12px 0 4px; font-size: 1.05em;">ノート（簡易エディタ）</h2>
        <div style="display: grid; gap: 8px;">
            <div style="opacity: 0.8; font-size: 0.95em;">
                クリックで選択 → pitchOffset を編集 →「ノートでレンダリング」
            </div>
            <svg
                width="720"
                height="160"
                viewBox="0 0 720 160"
                style="border: 1px solid rgba(0,0,0,0.2); background: rgba(0,0,0,0.02);"
                role="button"
                tabindex="0"
                aria-label="ノートを選択"
                on:click={(e) => {
                    const track = noteTrack;
                    if (!track) return;
                    const rect = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
                    const x = e.clientX - rect.left;
                    const t = (x / rect.width) * track.duration;
                    selectNoteByTime(t);
                }}
                on:keydown={(e) => {
                    if (e.key !== 'Enter' && e.key !== ' ') return;
                    e.preventDefault();
                    const track = noteTrack;
                    if (!track) return;
                    // キーボード操作時は現在選択を維持（必要なら後でカーソル導入）
                    if (!selectedNoteId) selectNoteByTime(0);
                }}
            >
                {#each noteTrack.notes as n (n.id)}
                    {@const x = (n.startTime / noteTrack.duration) * 720}
                    {@const w = ((n.endTime - n.startTime) / noteTrack.duration) * 720}
                    {@const y = 20 + (120 - (n.baseSemitone - 48) * 4)}
                    <rect
                        x={x}
                        y={Math.max(10, Math.min(130, y))}
                        width={Math.max(2, w)}
                        height="18"
                        opacity={n.enabled ? 1 : 0.3}
                        fill={n.id === selectedNoteId ? 'rgba(0, 120, 255, 0.55)' : 'rgba(0,0,0,0.25)'}
                        stroke="rgba(0,0,0,0.35)"
                    />
                {/each}
            </svg>

            {#if selectedNoteId}
                {@const note = noteTrack.notes.find((n) => n.id === selectedNoteId)}
                {#if note}
                    <div style="display: flex; gap: 12px; align-items: center; flex-wrap: wrap;">
                        <div>Selected: {note.startTime.toFixed(2)}s–{note.endTime.toFixed(2)}s</div>
                        <label>
                            pitchOffset(semitones):
                            <input
                                type="number"
                                step="0.1"
                                value={note.pitchOffset}
                                on:input={(e) => updateSelectedPitchOffset(Number((e.currentTarget as HTMLInputElement).value))}
                            />
                        </label>
                        <label>
                            enabled:
                            <input
                                type="checkbox"
                                checked={note.enabled}
                                on:change={(e) => updateSelectedEnabled((e.currentTarget as HTMLInputElement).checked)}
                            />
                        </label>
                    </div>
                {/if}
            {/if}
        </div>
    {/if}

    <p style="opacity: 0.8; font-size: 0.95em;">
        経路: AudioBufferSourceNode → AudioWorkletNode(melody-processor) → WASM(MelodyShifter.process_block) → destination
    </p>
</section>