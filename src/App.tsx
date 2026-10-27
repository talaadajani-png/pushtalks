import { useEffect, useState } from 'react';
import { api, on, type Settings, type Tick } from './api';
import Onboarding from './views/Onboarding';
import Meeting from './views/Meeting';
import Library from './views/Library';
import DictationView from './views/Dictation';
import SettingsView from './views/Settings';
import { clock, message } from './api';

type Tab = 'meeting' | 'library' | 'dictation' | 'settings';
const TABS: [Tab, string][] = [
  ['meeting', 'Meeting'],
  ['library', 'Library'],
  ['dictation', 'Dictation'],
  ['settings', 'Settings'],
];

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [tab, setTab] = useState<Tab>('meeting');
  const [openMeeting, setOpenMeeting] = useState<number | null>(null);
  const [tick, setTick] = useState<Tick | null>(null);
  const [fatal, setFatal] = useState('');
  // Bumped when the call prompt's "Record…" is clicked, so the Meeting tab opens the consent dialog.
  const [callRequest, setCallRequest] = useState<{ app: string | null; n: number } | null>(null);

  useEffect(() => {
    api.settingsGet().then(setSettings).catch((e) => setFatal(message(e)));
    const offNav = on<string>('navigate', (t) => setTab(t as Tab));
    const offTick = on<Tick>('meeting-tick', setTick);
    const offCall = on<string | null>('record-call', (app) => {
      setTab('meeting');
      setCallRequest((r) => ({ app, n: (r?.n ?? 0) + 1 }));
    });
    return () => {
      offNav();
      offTick();
      offCall();
    };
  }, []);

  if (fatal) return <div className="content error">PushTalks couldn't load its settings: {fatal}</div>;
  if (!settings) return null;

  if (!settings.onboarded) {
    return (
      <>
        <div className="titlebar" data-tauri-drag-region />
        <Onboarding
        onDone={async () => {
          setSettings(await api.settingsSave({ ...settings, onboarded: true }));
        }}
        />
      </>
    );
  }

  const showMeeting = (id: number) => {
    setOpenMeeting(id);
    setTab('library');
  };

  return (
    <div className="app">
      <div className="titlebar" data-tauri-drag-region />
      <nav className="tabs" data-tauri-drag-region>
        <div className="tabs-pill">
          {TABS.map(([id, label]) => (
            <button key={id} className={tab === id ? 'active' : ''} onClick={() => setTab(id)}>
              {label}
            </button>
          ))}
        </div>
        {tick?.recording && tab !== 'meeting' && (
          <span className="rec">
            <span className="dot pulse" /> Recording {clock(tick.elapsed_secs)}
          </span>
        )}
      </nav>
      <main className="content">
        {tab === 'meeting' && <Meeting tick={tick} onOpen={showMeeting} callRequest={callRequest} />}
        {tab === 'library' && <Library openId={openMeeting} onOpenChange={setOpenMeeting} />}
        {tab === 'dictation' && <DictationView settings={settings} />}
        {tab === 'settings' && <SettingsView settings={settings} onChange={setSettings} />}
      </main>
    </div>
  );
}
