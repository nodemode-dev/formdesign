// Lorem Fill: fills the selected empty text frames with placeholder text, as one undoable step.
(function () {
  formdes.registerCommand('fill', async function () {
    try {
      var st = (await formdes.query({ method: 'state' })).data || {};
      var picked = (st.selection && st.selection.items) || [];
      if (!picked.length) { formdes.notify('Select one or more empty text frames first'); return; }
      var ins = (await formdes.query({ method: 'inspect' })).data || {};
      var lengths = {};
      (ins.stories || []).forEach(function (s) { lengths[s.id] = s.length; });
      var empty = [];
      (ins.spreads || []).forEach(function (sp) {
        (sp.items || []).forEach(function (it) {
          if (picked.indexOf(it.id) >= 0 && it.kind === 'text frame' && it.story !== undefined && !lengths[it.story]) empty.push(it.id);
        });
      });
      if (!empty.length) { formdes.notify('None of the selected frames is an empty text frame'); return; }
      var r = await formdes.call('run_commands', { commands: [{ command: 'type.fillWithPlaceholder', params: { ids: empty } }] });
      if (r.isError) { formdes.notify('Not filled: ' + r.text); return; }
      formdes.notify('Filled ' + empty.length + ' frame' + (empty.length === 1 ? '' : 's'));
    } catch (e) {
      formdes.notify(String((e && e.message) || e));
    }
  });
})();
