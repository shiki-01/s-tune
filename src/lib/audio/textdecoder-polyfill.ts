// AudioWorkletGlobalScope では `TextDecoder` が無いことがある。
// wasm-bindgen の生成JSがトップレベルで TextDecoder を参照するため、
// 依存として先に評価されるよう別モジュールに切り出す。

if (typeof (globalThis as unknown as { TextDecoder?: unknown }).TextDecoder === 'undefined') {
	class MinimalTextDecoder {
		decode(input?: ArrayBufferView | ArrayBuffer): string {
			if (!input) return '';
			const u8 =
				input instanceof ArrayBuffer
					? new Uint8Array(input)
					: new Uint8Array(input.buffer, input.byteOffset, input.byteLength);

			let out = '';
			let i = 0;
			while (i < u8.length) {
				const c = u8[i++];
				if (c < 0x80) {
					out += String.fromCharCode(c);
					continue;
				}
				if (c < 0xe0) {
					const c2 = u8[i++] ?? 0;
					out += String.fromCharCode(((c & 0x1f) << 6) | (c2 & 0x3f));
					continue;
				}
				if (c < 0xf0) {
					const c2 = u8[i++] ?? 0;
					const c3 = u8[i++] ?? 0;
					out += String.fromCharCode(((c & 0x0f) << 12) | ((c2 & 0x3f) << 6) | (c3 & 0x3f));
					continue;
				}

				const c2 = u8[i++] ?? 0;
				const c3 = u8[i++] ?? 0;
				const c4 = u8[i++] ?? 0;
				let cp = ((c & 0x07) << 18) | ((c2 & 0x3f) << 12) | ((c3 & 0x3f) << 6) | (c4 & 0x3f);
				cp -= 0x10000;
				out += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
			}
			return out;
		}
	}

	(globalThis as unknown as { TextDecoder: unknown }).TextDecoder = MinimalTextDecoder;
}

// wasm-bindgen がトップレベルで TextEncoder も参照することがある
if (typeof (globalThis as unknown as { TextEncoder?: unknown }).TextEncoder === 'undefined') {
	class MinimalTextEncoder {
		encode(input = ''): Uint8Array {
			const bytes: number[] = [];
			for (let i = 0; i < input.length; i++) {
				const codePoint = input.codePointAt(i) ?? 0;
				// surrogate pair を消費
				if (codePoint > 0xffff) i++;

				if (codePoint <= 0x7f) {
					bytes.push(codePoint);
				} else if (codePoint <= 0x7ff) {
					bytes.push(0xc0 | (codePoint >> 6));
					bytes.push(0x80 | (codePoint & 0x3f));
				} else if (codePoint <= 0xffff) {
					bytes.push(0xe0 | (codePoint >> 12));
					bytes.push(0x80 | ((codePoint >> 6) & 0x3f));
					bytes.push(0x80 | (codePoint & 0x3f));
				} else {
					bytes.push(0xf0 | (codePoint >> 18));
					bytes.push(0x80 | ((codePoint >> 12) & 0x3f));
					bytes.push(0x80 | ((codePoint >> 6) & 0x3f));
					bytes.push(0x80 | (codePoint & 0x3f));
				}
			}
			return new Uint8Array(bytes);
		}

		encodeInto(input: string, dest: Uint8Array): { read: number; written: number } {
			const encoded = this.encode(input);
			const written = Math.min(dest.length, encoded.length);
			dest.set(encoded.subarray(0, written));
			// 厳密な read を出すのは重いので、最低限の互換として「全部読んだ」扱い
			return { read: input.length, written };
		}
	}

	(globalThis as unknown as { TextEncoder: unknown }).TextEncoder = MinimalTextEncoder;
}

export {};