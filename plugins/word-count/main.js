// Word Count: read-only. Counts the selected text, else the stories of the selected frames, else the story being edited.
(function () {
  var root = document.createElement('div');
  root.className = 'wc';
  root.innerHTML =
    '<style>' +
    '.wc{padding:12px;display:flex;flex-direction:column;gap:10px}' +
    '.wc-scope{font-size:11px;color:var(--fd-muted);min-height:14px}' +
    '.wc-grid{display:grid;grid-template-columns:1fr 1fr;gap:8px}' +
    '.wc-cell{border:1px solid var(--fd-line);border-radius:6px;background:var(--fd-surface);padding:8px 10px}' +
    '.wc-n{font-size:20px;font-weight:600;color:var(--fd-fg);font-variant-numeric:tabular-nums}' +
    '.wc-l{font-size:11px;color:var(--fd-muted);margin-top:2px}' +
    '.wc-btn{align-self:flex-start;border:1px solid var(--fd-line);background:var(--fd-surface);color:var(--fd-fg);border-radius:6px;padding:5px 12px;font:inherit;font-size:12px;cursor:pointer}' +
    '.wc-btn:hover{border-color:var(--fd-accent)}' +
    '</style>' +
    '<div class="wc-scope" id="scope" aria-live="polite">Nothing to count</div>' +
    '<div class="wc-grid">' +
    cell('words', 'Words') + cell('chars', 'Characters') + cell('nospace', 'Without spaces') +
    cell('sentences', 'Sentences') + cell('paras', 'Paragraphs') + cell('read', 'Reading time') +
    '</div>' +
    '<button class="wc-btn" id="go" type="button">Count again</button>';
  document.body.appendChild(root);

  function cell(id, label) {
    return '<div class="wc-cell"><div class="wc-n" id="' + id + '">0</div><div class="wc-l">' + label + '</div></div>';
  }
  function set(id, v) { var n = document.getElementById(id); if (n.textContent !== String(v)) n.textContent = String(v); }
  function slice(text, a, b) {
    var enc = new TextEncoder(), dec = new TextDecoder();
    return dec.decode(enc.encode(text).slice(Math.min(a, b), Math.max(a, b)));
  }
  function measure(text) {
    var words = (text.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu) || []).length;
    var sentences = (text.match(/[^.!?\n]+[.!?]+(\s|$)|[^.!?\n]+$/gm) || []).filter(function (s) { return /\S/.test(s); }).length;
    var paras = text.split(/[\r\n]/).filter(function (p) { return /\S/.test(p); }).length;
    var mins = words / 238;
    return { words: words, chars: Array.from(text).length, nospace: Array.from(text.replace(/\s/g, '')).length, sentences: sentences, paras: paras,
      read: words === 0 ? '0 sec' : mins < 1 ? Math.max(1, Math.round(mins * 60)) + ' sec' : Math.round(mins * 10) / 10 + ' min' };
  }
  async function story(id) {
    var r = await formdesign.query({ command: 'story.get', params: { story: id } });
    return (r.data && r.data.text) || '';
  }

  var running = false, again = false;
  async function count() {
    if (running) { again = true; return; }
    running = true;
    try {
      var st = (await formdesign.query({ method: 'state' })).data || {};
      var sel = st.selection || {}, text = '', scope = 'Nothing to count';
      if (sel.text && sel.text.story !== undefined) {
        var all = await story(sel.text.story);
        var a = sel.text.anchor, f = sel.text.focus === undefined ? a : sel.text.focus;
        if (a !== f) { text = slice(all, a, f); scope = 'Selected text'; }
        else { text = all; scope = 'Story being edited'; }
      } else if (sel.items && sel.items.length) {
        var ins = (await formdesign.query({ method: 'inspect' })).data || {};
        var frames = {};
        (ins.spreads || []).forEach(function (sp) { (sp.items || []).forEach(function (it) { frames[it.id] = it; }); });
        var seen = {}, parts = [];
        for (var i = 0; i < sel.items.length; i++) {
          var it = frames[sel.items[i]];
          if (it && it.story !== undefined && !seen[it.story]) { seen[it.story] = 1; parts.push(await story(it.story)); }
        }
        if (parts.length) { text = parts.join('\n'); scope = parts.length === 1 ? 'Story of the selected frame' : parts.length + ' stories of the selected frames'; }
      }
      var m = measure(text);
      ['words', 'chars', 'nospace', 'sentences', 'paras', 'read'].forEach(function (k) { set(k, m[k]); });
      var s = document.getElementById('scope');
      if (s.textContent !== scope) s.textContent = scope;
    } catch (e) {
      document.getElementById('scope').textContent = String(e.message || e);
    }
    running = false;
    if (again) { again = false; count(); }
  }

  document.getElementById('go').addEventListener('click', count);
  formdesign.registerCommand('refresh', function () { count(); formdesign.notify('Counted'); });
  formdesign.onChange(function () { count(); });
  count();
})();
