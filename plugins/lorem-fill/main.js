// Lorem Fill: fills the selected empty text frames with placeholder text, as one undoable step.
(function () {
  formdesign.registerCommand('fill', async function () {
    try {
      var st = (await formdesign.query({ method: 'state' })).data || {};
      var picked = (st.selection && st.selection.items) || [];
      if (!picked.length) { formdesign.notify('Select one or more empty text frames first'); return; }
      var ins = (await formdesign.query({ method: 'inspect' })).data || {};
      var lengths = {};
      (ins.stories || []).forEach(function (s) { lengths[s.id] = s.length; });
      var empty = [];
      (ins.spreads || []).forEach(function (sp) {
        (sp.items || []).forEach(function (it) {
          if (picked.indexOf(it.id) >= 0 && it.kind === 'text frame' && it.story !== undefined && !lengths[it.story]) empty.push(it.id);
        });
      });
      if (!empty.length) { formdesign.notify('None of the selected frames is an empty text frame'); return; }
      var r = await formdesign.call('run_commands', { commands: [{ command: 'type.fillWithPlaceholder', params: { ids: empty } }] });
      if (r.isError) { formdesign.notify('Not filled: ' + r.text); return; }
      formdesign.notify('Filled ' + empty.length + ' frame' + (empty.length === 1 ? '' : 's'));
    } catch (e) {
      formdesign.notify(String((e && e.message) || e));
    }
  });
})();
