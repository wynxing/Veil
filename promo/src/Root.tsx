import {Composition} from 'remotion';
import {Opening} from './Opening';

export const Root = () => (
  <Composition id="VeilOpening" component={Opening} durationInFrames={240}
    fps={30} width={1920} height={1080} />
);
