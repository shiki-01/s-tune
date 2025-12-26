function writeString(view: DataView, offset: number, s: string): void {
	for (let i = 0; i < s.length; i++) {
		view.setUint8(offset + i, s.charCodeAt(i));
	}
}

function floatToI16(x: number): number {
	const clamped = Math.max(-1, Math.min(1, x));
	return clamped < 0 ? Math.round(clamped * 0x8000) : Math.round(clamped * 0x7fff);
}

export function encodeWavMono16(samples: Float32Array, sampleRate: number): Blob {
	const numChannels = 1;
	const bitsPerSample = 16;
	const bytesPerSample = bitsPerSample / 8;
	const blockAlign = numChannels * bytesPerSample;
	const byteRate = sampleRate * blockAlign;

	const dataSize = samples.length * bytesPerSample;
	const headerSize = 44;
	const buffer = new ArrayBuffer(headerSize + dataSize);
	const view = new DataView(buffer);

	// RIFF header
	writeString(view, 0, 'RIFF');
	view.setUint32(4, 36 + dataSize, true);
	writeString(view, 8, 'WAVE');

	// fmt chunk
	writeString(view, 12, 'fmt ');
	view.setUint32(16, 16, true); // PCM
	view.setUint16(20, 1, true); // format=PCM
	view.setUint16(22, numChannels, true);
	view.setUint32(24, sampleRate, true);
	view.setUint32(28, byteRate, true);
	view.setUint16(32, blockAlign, true);
	view.setUint16(34, bitsPerSample, true);

	// data chunk
	writeString(view, 36, 'data');
	view.setUint32(40, dataSize, true);

	let o = 44;
	for (let i = 0; i < samples.length; i++) {
		view.setInt16(o, floatToI16(samples[i]), true);
		o += 2;
	}

	return new Blob([buffer], { type: 'audio/wav' });
}
