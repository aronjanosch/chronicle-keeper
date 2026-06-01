// Screen 05 — New Session. Upload Craig ZIP, label voices, set details, transcribe.
import { html, useState, useEffect, useRef } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, toneFor } from '../core.js';
import { createSession, fetchSession, uploadZip, saveSpeakers, saveSessionMetadata, runTranscribe, loadSession, openCampaign } from '../actions.js';

const EMPTY_META = { characters: [], locations: [], events: [], items: [], tags: [] };
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Sigil, Btn, Spinner } from '../ui.js';

function Waveform({ accent }) {
  const heights = Array.from({ length: 56 }, (_, i) => {
    const v = Math.sin(i * 1.3) * 0.4 + Math.cos(i * 0.7) * 0.3 + (((i * 31) % 13) / 13) * 0.4 + 0.2;
    return Math.max(0.08, Math.min(1, v));
  });
  return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 1.5, height: 24, flex: 1 }}>
    ${heights.map((h, i) => html`<div key=${i} style=${{ width: 2, height: `${h * 100}%`, background: accent ? (i % 5 === 0 ? 'var(--burgundy)' : 'var(--burgundy-300)') : 'var(--ink-ghost)', borderRadius: 1, opacity: accent ? 1 : 0.85 }} />`)}
  </div>`;
}

function TrackCard({ track, index, sp, roster, onChange }) {
  const assigned = !!(sp.player_name || sp.character_name);
  const inputStyle = { flex: 1, padding: '7px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, fontFamily: 'inherit', color: 'var(--ink)', minWidth: 0 };
  return html`<div style=${{ background: 'var(--surface)', border: `1px solid ${assigned ? 'var(--rule)' : 'var(--rule-strong)'}`, borderRadius: 8, padding: 14, display: 'flex', flexDirection: 'column', gap: 12 }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 10 }}>
      <div style=${{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--ink-faint)', minWidth: 60 }}>track ${String(track.id).padStart(2, '0')}</div>
      <div style=${{ flex: 1, minWidth: 0, fontSize: 11.5, color: 'var(--ink-muted)', fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>${track.filename}</div>
    </div>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 12, padding: '8px 12px', background: 'var(--paper)', border: '1px solid var(--rule-soft)', borderRadius: 6 }}>
      <div style=${{ width: 28, height: 28, borderRadius: 6, background: 'var(--paper-deep)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--ink-faint)' }}>
        ${assigned ? html`<${Sigil} ch=${(sp.character_name || sp.player_name || '?')[0].toUpperCase()} tone=${toneFor(sp.player_name || sp.character_name)} />` : html`<${Icon} name="waveform" size=${13} />`}
      </div>
      <${Waveform} accent=${assigned} />
    </div>
    <div style=${{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
      <input list="ck-roster" placeholder="Player" value=${sp.player_name} style=${inputStyle}
        onInput=${(e) => {
          const v = e.target.value;
          const match = roster.find((p) => (p.player_name || '').toLowerCase() === v.trim().toLowerCase());
          onChange({ ...sp, player_name: v, ...(match ? { character_name: sp.character_name || match.character_name || '', pronouns: sp.pronouns || match.pronouns || '' } : {}) });
        }} />
      <input placeholder="Character" value=${sp.character_name} style=${inputStyle}
        onInput=${(e) => onChange({ ...sp, character_name: e.target.value })} />
      <select value=${sp.pronouns} style=${{ ...inputStyle, flex: '0 0 110px', cursor: 'pointer' }}
        onChange=${(e) => onChange({ ...sp, pronouns: e.target.value })}>
        <option value="">pronouns</option>
        <option value="she/her">she/her</option>
        <option value="he/him">he/him</option>
        <option value="they/them">they/them</option>
      </select>
    </div>
    ${!assigned && roster.length ? html`<div style=${{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 11.5, color: 'var(--ink-muted)', flexWrap: 'wrap' }}>
      <${Icon} name="sparkle" size=${11} /> <span>From roster:</span>
      ${roster.slice(0, 5).map((p, i) => html`<button key=${i} onClick=${() => onChange({ ...sp, player_name: p.player_name || '', character_name: p.character_name || '', pronouns: p.pronouns || '' })} style=${{ display: 'inline-flex', alignItems: 'center', gap: 6, padding: '4px 9px 4px 4px', background: p.is_gm ? 'var(--ochre-50)' : 'var(--paper-deep)', border: `1px solid ${p.is_gm ? 'rgba(168,115,40,.3)' : 'var(--rule)'}`, borderRadius: 999, fontSize: 11.5, color: 'var(--ink-soft)', cursor: 'pointer' }}>
        <span style=${{ width: 16, height: 16, borderRadius: '50%', background: `var(--${toneFor(p.player_name)}-50)`, color: `var(--${toneFor(p.player_name)})`, fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 9, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>${(p.character_name || p.player_name || '?')[0].toUpperCase()}</span>
        ${p.is_gm ? `GM · ${p.player_name}` : `${p.player_name}${p.character_name ? ` · ${p.character_name}` : ''}`}
      </button>`)}
    </div>` : ''}
  </div>`;
}

export function NewSessionScreen({ store }) {
  const c = store.campaign;
  // When `attach` is set we're adding a recording to an existing session
  // (upload-later flow) rather than creating a fresh draft.
  const attachId = store.route?.params?.attach || null;
  const [sid, setSid] = useState(null);
  const [number, setNumber] = useState(c?.next_session_number || '');
  const [tracks, setTracks] = useState([]);
  const [speakers, setSpeakers] = useState({}); // track_id -> {player_name,character_name,pronouns}
  const [title, setTitle] = useState('');
  const [date, setDate] = useState(new Date().toISOString().slice(0, 10));
  const [notes, setNotes] = useState('');
  const [meta, setMeta] = useState(EMPTY_META); // preserved across save (edited on the session screen)
  const [uploading, setUploading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const fileRef = useRef(null);
  const sidRef = useRef(null);

  useEffect(() => {
    let live = true;
    (async () => {
      try {
        if (attachId) {
          const s = (store.session && store.session.session_id === attachId) ? store.session : await fetchSession(attachId);
          if (!live) return;
          const cam = s.campaign || {};
          sidRef.current = s.session_id;
          setSid(s.session_id);
          setNumber(cam.session_number || c?.next_session_number || '');
          setTitle(cam.title || '');
          setDate(cam.date || new Date().toISOString().slice(0, 10));
          setNotes(cam.notes || '');
          setMeta(s.metadata || EMPTY_META);
          const t = s.tracks || [];
          setTracks(t);
          const sp = {};
          (s.speakers || []).forEach((x) => { sp[x.track_id] = { track_id: x.track_id, player_name: x.player_name || '', character_name: x.character_name || '', pronouns: x.pronouns || '' }; });
          t.forEach((tr) => { if (!sp[tr.id]) sp[tr.id] = { track_id: tr.id, player_name: '', character_name: '', pronouns: '' }; });
          setSpeakers(sp);
        } else {
          // No backend row until first upload/save, so cancel leaves nothing behind.
          if (!live) return;
          sidRef.current = null;
          setSid(null);
          setNumber(c?.next_session_number || '');
          setTitle('');
          setDate(new Date().toISOString().slice(0, 10));
          setNotes('');
          setMeta(EMPTY_META);
          setTracks([]);
          setSpeakers({});
        }
      } catch (e) { if (live) setErr(e.message); }
    })();
    return () => { live = false; };
  }, [c?.campaign_id, attachId]);

  async function ensureSession() {
    if (sidRef.current) return sidRef.current;
    const created = await createSession();
    sidRef.current = created.session_id;
    setSid(created.session_id);
    if (created.session_number) setNumber((n) => n || created.session_number);
    return created.session_id;
  }

  const gmName = (c?.gm || '').trim();
  const roster = [
    ...(gmName ? [{ player_name: gmName, character_name: '', pronouns: c?.gm_pronouns || '', is_gm: true }] : []),
    ...(c?.players || []),
  ];
  const assignedCount = Object.values(speakers).filter((s) => s.player_name || s.character_name).length;

  async function onFile(e) {
    const file = e.target.files?.[0];
    if (!file) return;
    setUploading(true); setErr(null);
    try {
      const id = await ensureSession();
      const t = await uploadZip(id, file);
      setTracks(t);
      const init = {};
      t.forEach((tr) => { init[tr.id] = { track_id: tr.id, player_name: '', character_name: '', pronouns: '' }; });
      setSpeakers(init);
    } catch (e2) { setErr(e2.message); }
    finally { setUploading(false); }
  }

  function update(trackId, sp) { setSpeakers((m) => ({ ...m, [trackId]: { ...sp, track_id: trackId } })); }

  async function begin(transcribeNow) {
    if (transcribeNow && !tracks.length) { setErr('Upload a recording before transcribing.'); return; }
    setBusy(true); setErr(null);
    try {
      const id = await ensureSession();
      if (tracks.length) {
        await saveSpeakers(id, tracks.map((t) => speakers[t.id] || { track_id: t.id, player_name: '', character_name: '', pronouns: '' }));
      }
      await saveSessionMetadata({
        session_id: id, campaign_id: c.campaign_id, session_number: Number(number) || null,
        title: title.trim() || null, date: date || null,
        metadata: meta || EMPTY_META, notes: notes.trim() || null,
      });
      await loadSession(id);          // navigates to session screen
      if (transcribeNow) runTranscribe();  // fire-and-forget; banner shows progress
    } catch (e) { setErr(e.message); setBusy(false); }
  }

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="campaign" active="sessions" campaign=${c} />`}
    topbar=${html`<${Topbar} crumbs=${[
      { label: 'Campaigns', onClick: () => navigate('library') },
      c && { label: c.name, onClick: () => openCampaign(c.campaign_id) },
      attachId ? 'Add recording' : 'New session',
    ]} right=${html`
      <div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <${Btn} kind="ghost" onClick=${() => (attachId ? navigate('session', { id: attachId }) : navigate('campaign', { id: c?.campaign_id }))}>Cancel</${Btn}>
        <${Btn} kind="secondary" disabled=${busy} onClick=${() => begin(false)}>${tracks.length ? 'Save draft' : 'Save without recording'}</${Btn}>
        <${Btn} kind="primary" iconRight="arrow-r" disabled=${busy || !tracks.length} onClick=${() => begin(true)}>
          ${busy ? 'Saving…' : 'Begin transcription'}
        </${Btn}>
      </div>`} />`}
  >
    <datalist id="ck-roster">${roster.map((p, i) => html`<option key=${i} value=${p.player_name} />`)}</datalist>

    <div style=${{ marginBottom: 18 }}>
      <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>${attachId ? 'Add recording' : 'New session'} · ${c?.name}</div>
      <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 28, fontWeight: 500, letterSpacing: '-0.015em', color: 'var(--ink)', lineHeight: 1.15, marginTop: 2 }}>
        Session <span style=${{ color: 'var(--ink-muted)', fontStyle: 'italic' }}>#${number || '…'}</span>
      </h1>
    </div>

    ${err && html`<div style=${{ marginBottom: 14, padding: '10px 14px', background: 'var(--burgundy-50)', border: '1px solid rgba(122,46,31,.2)', borderRadius: 6, color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}

    <div style=${{ display: 'grid', gridTemplateColumns: '1fr 320px', gap: 16 }}>
      <div>
        ${!tracks.length ? html`
          <div onClick=${() => fileRef.current?.click()} style=${{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: 12, padding: '48px 24px', background: 'var(--surface)', border: '1.5px dashed var(--rule-strong)', borderRadius: 8, cursor: uploading ? 'default' : 'pointer', textAlign: 'center' }}>
            ${uploading ? html`<${Spinner} size=${22} />` : html`<div style=${{ width: 44, height: 44, borderRadius: 8, background: 'var(--paper-deep)', border: '1px solid var(--rule)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--burgundy)' }}><${Icon} name="upload" size=${18} /></div>`}
            <div style=${{ fontFamily: 'var(--font-display)', fontSize: 16, fontWeight: 500, color: 'var(--ink-soft)' }}>${uploading ? 'Unpacking recording…' : 'Drop a Craig recording'}</div>
            <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic', fontFamily: 'var(--font-display)' }}>A Craig Bot .zip with one audio track per speaker.</div>
            <input ref=${fileRef} type="file" accept=".zip" style=${{ display: 'none' }} onChange=${onFile} disabled=${uploading} />
          </div>` : html`
          <div style=${{ display: 'flex', alignItems: 'center', gap: 14, padding: '14px 16px', background: 'var(--moss-50)', border: '1px solid rgba(74,93,58,.22)', borderRadius: 8, marginBottom: 16 }}>
            <div style=${{ width: 36, height: 36, borderRadius: 8, background: 'var(--moss)', color: '#FBF6E9', display: 'flex', alignItems: 'center', justifyContent: 'center' }}><${Icon} name="check" size=${14} /></div>
            <div style=${{ flex: 1 }}>
              <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500, color: 'var(--moss)' }}>Recording received</div>
              <div style=${{ fontSize: 12, color: 'var(--ink-muted)', fontFamily: 'var(--font-mono)', marginTop: 1 }}>${tracks.length} track${tracks.length === 1 ? '' : 's'} extracted</div>
            </div>
            <${Btn} kind="ghost" size="sm" onClick=${() => fileRef.current?.click()}>Replace</${Btn}>
            <input ref=${fileRef} type="file" accept=".zip" style=${{ display: 'none' }} onChange=${onFile} />
          </div>

          <div style=${{ display: 'flex', alignItems: 'baseline', gap: 10, marginBottom: 10 }}>
            <h2 style=${{ fontFamily: 'var(--font-display)', fontSize: 18, fontWeight: 500 }}>Label the voices</h2>
            <span style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>· ${assignedCount} of ${tracks.length} labelled</span>
          </div>
          <div style=${{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            ${tracks.map((t, i) => html`<${TrackCard} key=${t.id} track=${t} index=${i} sp=${speakers[t.id] || {}} roster=${roster} onChange=${(sp) => update(t.id, sp)} />`)}
          </div>`}
      </div>

      <div style=${{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden' }}>
          <div style=${{ padding: '12px 14px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 8 }}>
            <${Icon} name="doc" size=${13} className="ck-ink-muted" />
            <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500 }}>Session details</h3>
          </div>
          <div style=${{ padding: 14, display: 'flex', flexDirection: 'column', gap: 11 }}>
            <div>
              <div style=${{ fontSize: 11.5, fontWeight: 500, color: 'var(--ink-soft)', marginBottom: 4 }}>Title</div>
              <input value=${title} onInput=${(e) => setTitle(e.target.value)} placeholder="The Vault Beneath…" style=${{ width: '100%', padding: '8px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, fontFamily: 'var(--font-display)', fontStyle: 'italic', color: 'var(--ink)' }} />
            </div>
            <div style=${{ display: 'flex', gap: 8 }}>
              <div style=${{ flex: 1 }}>
                <div style=${{ fontSize: 11.5, fontWeight: 500, color: 'var(--ink-soft)', marginBottom: 4 }}>Date</div>
                <input type="date" value=${date} onInput=${(e) => setDate(e.target.value)} style=${{ width: '100%', padding: '7px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, color: 'var(--ink)' }} />
              </div>
              <div style=${{ width: 84 }}>
                <div style=${{ fontSize: 11.5, fontWeight: 500, color: 'var(--ink-soft)', marginBottom: 4 }}>Session #</div>
                <input type="number" value=${number} onInput=${(e) => setNumber(e.target.value)} style=${{ width: '100%', padding: '7px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, fontFamily: 'var(--font-mono)', color: 'var(--ink)' }} />
              </div>
            </div>
            <div>
              <div style=${{ fontSize: 11.5, fontWeight: 500, color: 'var(--ink-soft)', marginBottom: 4 }}>Notes (optional)</div>
              <textarea value=${notes} onInput=${(e) => setNotes(e.target.value)} style=${{ width: '100%', minHeight: 72, padding: '8px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, lineHeight: 1.4, resize: 'vertical', fontFamily: 'inherit', color: 'var(--ink)' }}></textarea>
            </div>
          </div>
        </div>
      </div>
    </div>
  </${Shell}>`;
}
