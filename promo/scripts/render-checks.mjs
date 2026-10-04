import {bundle} from '@remotion/bundler';
import {selectComposition, renderStill} from '@remotion/renderer';
import {mkdirSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
const entryPoint = fileURLToPath(new URL('../src/index.ts', import.meta.url));
const out = fileURLToPath(new URL('../out/checks/', import.meta.url));
mkdirSync(out, {recursive: true});
const serveUrl = await bundle({entryPoint});
const composition = await selectComposition({serveUrl, id: 'VeilPromo'});
if (composition.durationInFrames !== 1350 || composition.fps !== 30) throw Error('Unexpected composition duration.');
for (const [label, frame] of [['intro', 60], ['shock', 280], ['rewind', 345], ['brand', 435], ['select', 525],
  ['off', 610], ['restore', 717], ['night', 840], ['everyday', 1000], ['return', 1170], ['end', 1310]]) {
  await renderStill({serveUrl, composition, frame, scale: .5, imageFormat: 'png', output: `${out}/${label}.png`});
  console.log(`Checked frame ${frame}: ${label}`);
}
