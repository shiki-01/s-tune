export interface HarmonicProfile {
	noteId: string; // NoteSegment.id
	harmonics: number[]; // harmonic 1..N linear gain (1.0 = 0dB)
}

export interface TrackMeanSpectrum {
	harmonics: number[]; // track-global harmonic gains
}

export type HarmonicsConfig = {
	count: number;
	defaultGain: number;
	minGain: number;
	maxGain: number;
};

export const DEFAULT_HARMONICS_CONFIG: HarmonicsConfig = {
	count: 16,
	defaultGain: 1.0,
	minGain: 0.25,
	maxGain: 2.0
};

export function createDefaultHarmonics(config: HarmonicsConfig = DEFAULT_HARMONICS_CONFIG): number[] {
	return Array.from({ length: config.count }, () => config.defaultGain);
}
