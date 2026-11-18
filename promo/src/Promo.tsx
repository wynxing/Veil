import {Audio} from '@remotion/media';
import {AbsoluteFill, staticFile} from 'remotion';
import {TransitionSeries, linearTiming} from '@remotion/transitions';
import {fade} from '@remotion/transitions/fade';
import {Opening} from './Opening';
import {Surprise} from './scenes/Surprise';
import {Product} from './scenes/Product';
import {Night} from './scenes/Night';
import {Everyday} from './scenes/Everyday';
import {Return} from './scenes/Return';
import {End} from './scenes/End';

export const Promo = () => <AbsoluteFill>
  <Audio src={staticFile('promo.wav')} />
  <TransitionSeries>
    <TransitionSeries.Sequence durationInFrames={252}><Opening sample={false} /></TransitionSeries.Sequence>
    <TransitionSeries.Transition presentation={fade()} timing={linearTiming({durationInFrames: 12})} />
    <TransitionSeries.Sequence durationInFrames={162}><Surprise /></TransitionSeries.Sequence>
    <TransitionSeries.Transition presentation={fade()} timing={linearTiming({durationInFrames: 12})} />
    <TransitionSeries.Sequence durationInFrames={372}><Product /></TransitionSeries.Sequence>
    <TransitionSeries.Transition presentation={fade()} timing={linearTiming({durationInFrames: 12})} />
    <TransitionSeries.Sequence durationInFrames={177}><Night /></TransitionSeries.Sequence>
    <TransitionSeries.Transition presentation={fade()} timing={linearTiming({durationInFrames: 12})} />
    <TransitionSeries.Sequence durationInFrames={177}><Everyday /></TransitionSeries.Sequence>
    <TransitionSeries.Transition presentation={fade()} timing={linearTiming({durationInFrames: 12})} />
    <TransitionSeries.Sequence durationInFrames={162}><Return /></TransitionSeries.Sequence>
    <TransitionSeries.Transition presentation={fade()} timing={linearTiming({durationInFrames: 12})} />
    <TransitionSeries.Sequence durationInFrames={120}><End /></TransitionSeries.Sequence>
  </TransitionSeries>
</AbsoluteFill>;
