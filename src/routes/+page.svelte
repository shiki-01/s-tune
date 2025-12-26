<script lang="ts">
    import { browser } from '$app/environment';
    import { onDestroy } from 'svelte';

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
        <button on:click={play} disabled={!loadedBuffer}>再生</button>
        <button on:click={stop}>停止</button>
    </div>

    <p style="opacity: 0.8; font-size: 0.95em;">
        経路: AudioBufferSourceNode → AudioWorkletNode(melody-processor) → WASM(MelodyShifter.process_block) → destination
    </p>
</section>