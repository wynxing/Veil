import {mkdirSync, writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';

// Original, deterministic 8-second cue; no sampled recordings or external music.
const rate = 48000;
const seconds = 8;
const samples = new Float64Array(rate * seconds);
const tau = Math.PI * 2;
let seed = 7841;
const noise = () => {seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296 * 2 - 1;};
const freq = (midi) => 440 * 2 ** ((midi - 69) / 12);

function tone(at, duration, midi, gain = 0.1, type = 'bell') {
  const start = Math.round(at * rate);
  for (let i = 0; i < duration * rate && start + i < samples.length; i++) {
    const t = i / rate;
    const f = freq(midi);
    const attack = Math.min(t / 0.008, 1);
    const release = Math.min((duration - t) / 0.05, 1);
    let value = Math.sin(tau * f * t);
    if (type === 'bell') value = value * 0.75 + Math.sin(tau * f * 2 * t) * 0.18 + Math.sin(tau * f * 3 * t) * 0.07;
    samples[start + i] += value * attack * release * Math.exp(-t * (type === 'bass' ? 7 : 5)) * gain;
  }
}

const beat = 60 / 116;
const melody = [76, 79, 83, 79, 81, 79, 76, 74, 76, 79, 83, 86, 83, 79];
melody.forEach((note, index) => tone(0.1 + index * beat / 2, 0.38, note, index % 4 === 0 ? 0.13 : 0.085));
[52, 52, 57, 57, 55, 55, 59].forEach((note, index) => tone(0.1 + index * beat, 0.32, note, 0.1, 'bass'));
for (let n = 0; n < 14; n++) {
  const start = Math.round((0.1 + n * beat / 2) * rate);
  for (let i = 0; i < 0.035 * rate; i++) samples[start + i] += noise() * 0.019 * Math.exp(-i / rate * 120);
}

// Camera pull-out whoosh, followed by a comic pause and three caution notes.
for (let i = 0; i < 0.5 * rate; i++) {
  const t = i / rate;
  const envelope = Math.sin(Math.PI * t / 0.5) ** 2;
  samples[Math.round(2.85 * rate) + i] += noise() * envelope * 0.055;
}
tone(4.03, 0.13, 88, 0.16);
tone(4.21, 0.13, 88, 0.14);
tone(4.39, 0.26, 76, 0.16);
tone(4.72, 0.11, 52, 0.19, 'bass');
tone(6.36, 0.55, 72, 0.10);
tone(6.48, 0.5, 79, 0.09);
tone(6.60, 0.7, 84, 0.08);

const pcm = Buffer.alloc(samples.length * 2);
let peak = 0;
for (const sample of samples) peak = Math.max(peak, Math.abs(sample));
const multiplier = 0.74 / Math.max(peak, 0.001);
for (let i = 0; i < samples.length; i++) pcm.writeInt16LE(Math.round(samples[i] * multiplier * 32767), i * 2);
const header = Buffer.alloc(44);
header.write('RIFF', 0); header.writeUInt32LE(36 + pcm.length, 4); header.write('WAVEfmt ', 8);
header.writeUInt32LE(16, 16); header.writeUInt16LE(1, 20); header.writeUInt16LE(1, 22);
header.writeUInt32LE(rate, 24); header.writeUInt32LE(rate * 2, 28); header.writeUInt16LE(2, 32);
header.writeUInt16LE(16, 34); header.write('data', 36); header.writeUInt32LE(pcm.length, 40);
const directory = fileURLToPath(new URL('../public/', import.meta.url));
mkdirSync(directory, {recursive: true});
writeFileSync(`${directory}/opening.wav`, Buffer.concat([header, pcm]));
console.log('Generated original 8 s / 48 kHz PCM cue.');
