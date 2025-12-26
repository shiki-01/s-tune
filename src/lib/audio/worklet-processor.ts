
import { init, MelodyShifter } from './worklet-wasm';

type MelodyMessage = { type: 'semitones'; value: number };

class MelodyProcessor extends AudioWorkletProcessor {
	private _ready = false;
	private _semitones = 0;
	private _shifter: MelodyShifter | null = null;
	private _initPromise: Promise<void>;

	constructor(options?: AudioWorkletNodeOptions) {
		super();

		const processorOptions = options?.processorOptions as
			| { sampleRate?: number; wasmBytes?: ArrayBuffer }
			| undefined;
		const sr = processorOptions?.sampleRate ?? sampleRate;
		const wasmBytes = processorOptions?.wasmBytes;

		this._initPromise = (async () => {
			try {
				// WorkletGlobalScope では URL が無いことがあるため、
				// メインスレッドで取得した wasmBytes があればそれで初期化する。
				if (wasmBytes) {
					await init({ module_or_path: wasmBytes });
				} else {
					await init();
				}
				this._shifter = new MelodyShifter(sr);
				this._ready = true;
			} catch (e) {
				this._ready = false;
				this._shifter = null;
				console.error('[MelodyProcessor] WASM init failed:', e);
			}
		})();

		this.port.onmessage = (ev: MessageEvent<MelodyMessage>) => {
			const msg = ev.data;
			if (!msg || typeof msg !== 'object') return;

			if (msg.type === 'semitones') {
				const v = Number(msg.value);
				if (Number.isFinite(v)) this._semitones = v;
				return;
			}
		};
	}

	process(inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
		const input = inputs?.[0]?.[0];
		const output = outputs?.[0]?.[0];
		if (!input || !output) return true;

		// WASM準備できるまではバイパス
		if (!this._ready || !this._shifter) {
			output.set(input);
			return true;
		}

		// ストリーミング向けなので、1量子(通常128)ごとにそのまま処理する
		const buf = new Float32Array(input);
		this._shifter.process_block(buf, this._semitones);
		output.set(buf);
		return true;
	}
}

registerProcessor('melody-processor', MelodyProcessor);

