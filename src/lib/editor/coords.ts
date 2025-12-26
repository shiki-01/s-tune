export function clamp(v: number, min: number, max: number): number {
	if (!Number.isFinite(v)) return min;
	return Math.max(min, Math.min(max, v));
}

export function xToTime(x: number, width: number, duration: number): number {
	if (width <= 0 || duration <= 0) return 0;
	return clamp((x / width) * duration, 0, duration);
}

export function timeToX(time: number, width: number, duration: number): number {
	if (duration <= 0) return 0;
	return clamp((time / duration) * width, 0, width);
}

export function yToSemitone(y: number, height: number, minSemitone: number, maxSemitone: number): number {
	const range = maxSemitone - minSemitone;
	if (height <= 0 || range <= 0) return minSemitone;
	const rowH = height / (range + 1);
	const idx = Math.round(y / rowH);
	const semi = maxSemitone - idx;
	return clamp(semi, minSemitone, maxSemitone);
}

export function semitoneToYTop(semitone: number, height: number, minSemitone: number, maxSemitone: number): number {
	const range = maxSemitone - minSemitone;
	if (height <= 0 || range <= 0) return 0;
	const rowH = height / (range + 1);
	const idx = maxSemitone - semitone;
	return clamp(idx * rowH, 0, height);
}

export function snapTime(time: number, bpm: number, enabled: boolean): number {
	if (!enabled) return time;
	if (!Number.isFinite(bpm) || bpm <= 0) return time;
	const step = 60 / bpm; // 1 beat (quarter note)
	if (step <= 0) return time;
	return Math.round(time / step) * step;
}

export function getLocalPoint(evt: PointerEvent, element: Element): { x: number; y: number; width: number; height: number } {
	const rect = element.getBoundingClientRect();
	return {
		x: evt.clientX - rect.left,
		y: evt.clientY - rect.top,
		width: rect.width,
		height: rect.height
	};
}
