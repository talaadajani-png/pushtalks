import { useEffect, useState } from 'react';
import { api, on } from '../api';

// Shown when another app starts using the mic. "Record" only opens the consent dialog.
export default function CallPrompt() {
  const [app, setApp] = useState<string | null>(null);
  useEffect(() => {
    api.callPromptState().then(setApp);
    return on<string | null>('call-detected', setApp);
  }, []);
  return (
    <div className="card-prompt">
      <div>
        <b>On a call?</b>
        <div className="sub">{app ? `${app} is using your microphone.` : 'Another app is using your microphone.'} Record it with PushTalks?</div>
      </div>
      <div className="actions">
        <button onClick={() => api.callPromptDismiss()}>Not now</button>
        <button className="primary" onClick={() => api.callPromptRecord()}>Record…</button>
      </div>
    </div>
  );
}
