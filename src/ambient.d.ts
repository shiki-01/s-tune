declare module '*?url' {
    const url: string;
    export default url;
}

// AudioWorkletGlobalScope の型（TSのlib設定次第で入らないことがあるため最小限定義）
declare global {
    const sampleRate: number;

    abstract class AudioWorkletProcessor {
        readonly port: MessagePort;
        constructor(options?: AudioWorkletNodeOptions);
        process(
            inputs: Float32Array[][],
            outputs: Float32Array[][],
            parameters: Record<string, Float32Array>
        ): boolean;
    }

    function registerProcessor(
        name: string,
        processorCtor: (new (options?: AudioWorkletNodeOptions) => AudioWorkletProcessor) & {
            prototype: AudioWorkletProcessor;
        }
    ): void;
}

export {};

// wasm-pack生成物の型が拾えない場合の保険（dev/buildで型エラーを避ける）
declare module 'melody-dsp' {
    export default function init(
        arg?: unknown | { module_or_path?: unknown; module?: unknown }
    ): Promise<void>;

    export class MelodyShifter {
        constructor(sample_rate: number);
        process_block(input: Float32Array, semitones: number): void;
        readonly sample_rate: number;
    }

    export class MelodyEngine {
        constructor(sample_rate: number);
        set_notes(note_starts: Float32Array, note_ends: Float32Array, note_offsets: Float32Array): void;
        process_buffer(input: Float32Array): void;
        readonly sample_rate: number;
    }
}
