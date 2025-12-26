// IMPORTANT: import order matters.
// This module ensures TextDecoder is defined before wasm-bindgen glue runs.
import './textdecoder-polyfill';

import init, { MelodyShifter } from 'melody-dsp';

export { init, MelodyShifter };
