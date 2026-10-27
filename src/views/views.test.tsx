// Component tests with the Tauri commands mocked. They check the flows that guard PRD rules:
// consent before recording, and the learned-vocabulary prompt after an edit.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
const responses: Record<string, unknown> = {};

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args });
    const r = responses[cmd];
    return typeof r === 'function' ? (r as (a?: Record<string, unknown>) => unknown)(args) : r;
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import Meeting from './Meeting';
import Library from './Library';

const meeting = {
  row: { id: 7, title: 'Weekly sync', started_at: '2026-10-20T14:00:00Z', duration_secs: 600, md_path: '/x.md', audio_dir: null },
  segments: [
    { start: 4, end: 6, speaker: 'Me', text: 'Thanks Stefan.', words: [] },
    { start: 7, end: 9, speaker: 'Speaker 1', text: 'Stefan will send it.', words: [] },
  ],
  asr_model: 'parakeet-tdt-0.6b-v2-int8',
  speakers: ['Me', 'Speaker 1'],
  suggestions: [],
};

beforeEach(() => {
  calls.length = 0;
  Object.assign(responses, {
    meeting_recording: false,
    library_list: [],
    recordings_unfinished: [],
    processing_status: { current: null, queued: 0 },
    meeting_start: undefined,
    meeting_get: meeting,
    meeting_update: (a?: Record<string, unknown>) => ({
      ...meeting,
      segments: (a!.segments as { text: string; speaker: string }[]).map((s, i) => ({ ...meeting.segments[i], ...s })),
    }),
    vocab_correction: { heard: 'Stefan', spelling: 'Stephan', common: false },
    vocab_add: { id: 3, heard: 'Stefan', spelling: 'Stephan', mode: 'auto' },
    meeting_apply_rule: { meeting, changed: 1 },
  });
});
afterEach(cleanup);

describe('Meeting', () => {
  it('starts recording only after the consent box is ticked', async () => {
    const user = userEvent.setup();
    render(<Meeting tick={null} onOpen={() => {}} />);
    await user.click(screen.getByRole('button', { name: 'Start recording' }));
    const start = screen.getAllByRole('button', { name: 'Start recording' }).at(-1)!;
    expect((start as HTMLButtonElement).disabled).toBe(true);
    expect(calls.some((c) => c.cmd === 'meeting_start')).toBe(false);
    await user.click(screen.getByLabelText(/Everyone in this meeting knows/));
    expect((start as HTMLButtonElement).disabled).toBe(false);
    await user.click(start);
    const call = calls.find((c) => c.cmd === 'meeting_start');
    expect(call?.args).toEqual({ title: null, consentConfirmed: true, micOnly: false });
  });
});

describe('Library transcript editor', () => {
  it('saves an edit, offers to remember the spelling, and applies it on Always', async () => {
    const user = userEvent.setup();
    render(<Library openId={7} onOpenChange={() => {}} />);
    await user.click(await screen.findByText('Thanks Stefan.'));
    const box = screen.getByLabelText('Edit line');
    await user.clear(box);
    await user.type(box, 'Thanks Stephan.{Enter}');
    await waitFor(() => expect(calls.some((c) => c.cmd === 'meeting_update')).toBe(true));
    const update = calls.find((c) => c.cmd === 'meeting_update')!;
    expect((update.args!.segments as { text: string }[])[0].text).toBe('Thanks Stephan.');
    expect(calls.find((c) => c.cmd === 'vocab_correction')?.args).toEqual({ old: 'Thanks Stefan.', new: 'Thanks Stephan.' });
    await user.click(await screen.findByRole('button', { name: 'Always' }));
    await waitFor(() => expect(calls.some((c) => c.cmd === 'meeting_apply_rule')).toBe(true));
    expect(calls.find((c) => c.cmd === 'meeting_apply_rule')?.args).toEqual({ id: 7, ruleId: 3, segment: null });
    expect(await screen.findByText(/fixed 1 more place here/)).toBeTruthy();
  });

  it('does nothing when the text is unchanged', async () => {
    const user = userEvent.setup();
    render(<Library openId={7} onOpenChange={() => {}} />);
    await user.click(await screen.findByText('Thanks Stefan.'));
    await user.keyboard('{Enter}');
    expect(calls.some((c) => c.cmd === 'meeting_update')).toBe(false);
  });
});
