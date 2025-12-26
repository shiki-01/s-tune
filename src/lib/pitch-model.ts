export interface PitchFrame {
	time: number; // seconds
	f0: number | null; // Hz (null = unvoiced)
	confidence: number; // 0..1
}

export interface DetectedNote {
	id: string;
	startTime: number;
	endTime: number;
	midi: number; // estimated MIDI note number
	confidence: number; // 0..1
}
