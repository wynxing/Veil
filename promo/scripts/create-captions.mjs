import {readFileSync, mkdirSync, writeFileSync} from 'node:fs';
const cues = JSON.parse(readFileSync(new URL('../src/captions.json', import.meta.url), 'utf8'));
const stamp = sec => {
  const ms = Math.round(sec * 1000);
  return `${String(Math.floor(ms / 3600000)).padStart(2, '0')}:${String(Math.floor(ms / 60000) % 60).padStart(2, '0')}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')},${String(ms % 1000).padStart(3, '0')}`;
};
let prior = 0;
for (const cue of cues) {
  if (cue.start < prior || cue.end <= cue.start || cue.end > 45) throw Error('Invalid subtitle timing.');
  prior = cue.end;
}
mkdirSync(new URL('../out/', import.meta.url), {recursive: true});
writeFileSync(new URL('../out/veil-promo.srt', import.meta.url), '\uFEFF' + cues.map((cue, i) =>
  `${i + 1}\n${stamp(cue.start)} --> ${stamp(cue.end)}\n${cue.text}\n`).join('\n'), 'utf8');
console.log(`Exported ${cues.length} UTF-8 subtitle cues.`);
