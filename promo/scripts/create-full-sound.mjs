import {mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';

// Deterministic original synth score. No samples except our own existing cue.
const rate = 48000;
const length = 45 * rate;
const left = new Float64Array(length);
const right = new Float64Array(length);
const tau = 2 * Math.PI;
let seed = 930447;
const noise = () => {seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296 * 2 - 1;};
const hz = midi => 440 * 2 ** ((midi - 69) / 12);
const add = (i, value, pan = 0) => {
  if (i < 0 || i >= length) return;
  left[i] += value * (1 - pan * .4);
  right[i] += value * (1 + pan * .4);
};

function note(at, duration, midi, gain, type = 'pluck', pan = 0) {
  const start = Math.round(at * rate);
  const freq = hz(midi);
  for (let j = 0; j < duration * rate && start + j < length; j++) {
    const t = j / rate;
    const attack = Math.min(t / (type === 'pad' ? .15 : .008), 1);
    const release = Math.min((duration - t) / .12, 1);
    const decay = type === 'pad' ? .7 : type === 'bass' ? 7 : 4.8;
    const wave = Math.sin(tau * freq * t) * .78 + Math.sin(tau * freq * 2 * t) * .15 + Math.sin(tau * freq * 3 * t) * .07;
    const value = wave * gain * attack * release * Math.exp(-t * decay);
    add(start + j, value, pan);
    if (type === 'pluck') add(start + j + 6720, value * .18, -pan);
  }
}

function drum(at, kind, gain) {
  const start = Math.round(at * rate);
  const duration = kind === 'kick' ? .16 : .08;
  for (let j = 0; j < duration * rate; j++) {
    const t = j / rate;
    const signal = kind === 'kick' ? Math.sin(tau * (55 * t + 65 * .028 * (1 - Math.exp(-t / .028)))) : noise();
    add(start + j, signal * gain * Math.exp(-t * (kind === 'kick' ? 25 : 75)));
  }
}

function whoosh(at, duration, gain = .045) {
  for (let j = 0; j < duration * rate; j++) {
    const t = j / rate;
    add(Math.round(at * rate) + j, noise() * gain * Math.sin(Math.PI * t / duration) ** 2, Math.sin(t * 9));
  }
}

const original = readFileSync(new URL('../public/opening.wav', import.meta.url));
for (let j = 0; j < 8 * rate; j++) add(j, original.readInt16LE(44 + j * 2) / 32768 * .55);
note(8.35, .26, 79, .1); note(8.63, .36, 83, .08);
note(9.25, .17, 60, .15, 'bass'); note(9.48, .17, 55, .12, 'bass');
for (let j = 0; j < .7 * rate; j++) {
  const t = j / rate;
  add(Math.round(10.17 * rate) + j, (noise() * .035 + Math.sin(tau * (900 * t - 550 * t * t)) * .065)
    * Math.sin(Math.PI * t / .7), Math.sin(t * 12));
}
note(11.45, .5, 72, .09); note(11.7, .6, 79, .07);

const beat = 60 / 116;
const motif = [76, 79, 83, 79, 81, 79, 76, 74, 76, 79, 83, 86, 83, 79, 76, 79];
const chords = [[60, 64, 67], [57, 60, 64], [53, 57, 60], [55, 59, 62]];
function theme(start, end, quiet = false) {
  let n = 0;
  for (let at = start; at < end - .35; at += beat / 2, n++) {
    const chord = chords[Math.floor(n / 16) % chords.length];
    const melody = motif[n % motif.length] + (Math.floor(n / 32) % 2 ? -12 : 0);
    if (!quiet || n % 2 === 0) note(at, .45, melody, quiet ? .037 : .062, 'pluck', n % 2 ? .5 : -.5);
    if (n % 2 === 0) note(at, .38, chord[0] - 12, quiet ? .035 : .075, 'bass');
    if (n % 8 === 0) chord.forEach((pitch, i) => note(at, beat * 3.4, pitch, quiet ? .016 : .019, 'pad', (i - 1) * .5));
    if (!quiet) {
      drum(at, n % 4 === 0 ? 'kick' : 'hat', n % 4 === 0 ? .11 : .013);
      if (n % 4 === 2) drum(at, 'snare', .024);
    }
  }
}
theme(13.15, 24.82);
theme(25.2, 30.3, true);
theme(30.68, 35.85);
theme(36.18, 40.8);
[13.08, 25.02, 30.51, 36.03, 41.04].forEach(at => whoosh(at, .32, .024));
[18.10, 22.73].forEach(at => {note(at, .12, 84, .16); drum(at, 'kick', .055);});
[18.58, 23.50, 32.40].forEach(at => note(at, .3, 79, .07));
[60, 64, 67, 72].forEach((pitch, i) => note(41.15 + i * .13, 2.8, pitch, .053, 'pad', (i - 1.5) * .25));
note(41.48, 1.1, 84, .08); note(42.05, 1.0, 79, .055); note(42.58, 1.6, 76, .055);

let peak = 0;
for (let i = 0; i < length; i++) peak = Math.max(peak, Math.abs(left[i]), Math.abs(right[i]));
const gain = .78 / Math.max(peak, .001);
const pcm = Buffer.alloc(length * 4);
for (let i = 0; i < length; i++) {
  const fade = Math.min(1, (length - 1 - i) / (rate * .6));
  pcm.writeInt16LE(Math.round(left[i] * gain * fade * 32767), i * 4);
  pcm.writeInt16LE(Math.round(right[i] * gain * fade * 32767), i * 4 + 2);
}
const header = Buffer.alloc(44);
header.write('RIFF', 0); header.writeUInt32LE(36 + pcm.length, 4); header.write('WAVEfmt ', 8);
header.writeUInt32LE(16, 16); header.writeUInt16LE(1, 20); header.writeUInt16LE(2, 22);
header.writeUInt32LE(rate, 24); header.writeUInt32LE(rate * 4, 28); header.writeUInt16LE(4, 32);
header.writeUInt16LE(16, 34); header.write('data', 36); header.writeUInt32LE(pcm.length, 40);
const directory = fileURLToPath(new URL('../public/', import.meta.url));
mkdirSync(directory, {recursive: true});
writeFileSync(`${directory}/promo.wav`, Buffer.concat([header, pcm]));
console.log('Generated original 45 s / 48 kHz stereo synth score with timed cues.');
