import type { Correction } from '../api';

// "Always write X as Y?" after an edit (PRD §6.7).
export default function CorrectionPrompt({
  correction,
  onAlways,
  onOnce,
  onNever,
}: {
  correction: Correction;
  onAlways: () => void;
  onOnce: () => void;
  onNever: () => void;
}) {
  return (
    <div className="toast stack" role="dialog" aria-label="Remember this spelling">
      <div>
        Always write <b>“{correction.heard}”</b> as <b>“{correction.spelling}”</b>?
        {correction.common && (
          <div className="faint">
            “{correction.heard}” is a common word, so PushTalks will ask before changing it in future transcripts.
          </div>
        )}
      </div>
      <div className="row">
        <button className="primary" onClick={onAlways}>Always</button>
        <button onClick={onOnce}>Just this once</button>
        <button onClick={onNever}>Never ask for this word</button>
      </div>
    </div>
  );
}
