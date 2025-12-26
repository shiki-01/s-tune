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
    export default function init(moduleOrPath?: unknown): Promise<void>;

    export class MelodyShifter {
        constructor(sample_rate: number);
        process_block(input: Float32Array, semitones: number): void;
        readonly sample_rate: number;
    }
}
