import { createRoot } from 'react-dom/client';
import '@fontsource/work-sans/300.css';
import '@fontsource/work-sans/400.css';
import '@fontsource/work-sans/500.css';
import '@fontsource/work-sans/600.css';
import App from './App';
import Indicator from './views/Indicator';
import CallPrompt from './views/CallPrompt';

const hash = window.location.hash;
// The two small floating windows share the transparent pill styles.
if (hash === '#indicator' || hash === '#callprompt') {
  void import('./styles/indicator.css');
} else {
  void import('./styles/app.css');
}

// Show startup errors instead of a blank window.
window.addEventListener('error', (e) => {
  const root = document.getElementById('root');
  if (root && !root.hasChildNodes()) root.textContent = `PushTalks hit an error: ${e.message}`;
});

const view = hash === '#indicator' ? <Indicator /> : hash === '#callprompt' ? <CallPrompt /> : <App />;
createRoot(document.getElementById('root')!).render(view);
