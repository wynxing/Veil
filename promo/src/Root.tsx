import {Composition, Still} from 'remotion';
import {Opening} from './Opening';
import {Promo} from './Promo';
import {Cover} from './Cover';

export const Root = () => (<>
  <Composition id="VeilOpening" component={Opening} durationInFrames={240}
    fps={30} width={1920} height={1080} />
  <Composition id="VeilPromo" component={Promo} durationInFrames={1350}
    fps={30} width={1920} height={1080} />
  <Still id="VeilCover" component={Cover} width={1920} height={1080} />
</>);
