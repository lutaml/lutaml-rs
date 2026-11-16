// Namespace-resolved element tree of a small XML subset (no DTD, comments or PIs),
// in the shape of the Ruby oracle's `tree`: [[ns, name], [[[ns, name], value]...], children].
// Hand-rolled so Node and the browser compare identically; unsupported input throws.
const ENTITIES = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };
const unescape = s => s.replace(/&(amp|lt|gt|quot|apos);/g, (_, n) => ENTITIES[n]);
export function xmlTree(xml) {
  let pos = 0;
  const fail = what => { throw Error(`xmlTree: ${what} at ${pos}`); };
  function element(scopeIn) {
    if (xml[pos] !== '<') fail('element expected');
    const head = /^<([A-Za-z_][\w.-]*(?::[A-Za-z_][\w.-]*)?)/.exec(xml.slice(pos)) ?? fail('bad element name');
    pos += head[0].length;
    const raw = [], scope = { ...scopeIn };
    for (;;) {
      const attr = /^\s+([A-Za-z_][\w.:-]*)="([^"]*)"/.exec(xml.slice(pos));
      if (!attr) break;
      pos += attr[0].length;
      if (attr[1] === 'xmlns') scope[''] = unescape(attr[2]);
      else if (attr[1].startsWith('xmlns:')) scope[attr[1].slice(6)] = unescape(attr[2]);
      else raw.push([attr[1], unescape(attr[2])]);
    }
    const resolve = (qname, isAttr) => {
      const i = qname.indexOf(':');
      if (i < 0) return [isAttr ? '' : scope[''] ?? '', qname];
      const ns = scope[qname.slice(0, i)];
      if (ns === undefined) fail(`unbound prefix ${qname}`);
      return [ns, qname.slice(i + 1)];
    };
    const attrs = raw.map(([k, v]) => [resolve(k, true), v]).sort((a, b) => (a[0][0] + '\0' + a[0][1]).localeCompare(b[0][0] + '\0' + b[0][1]));
    const name = resolve(head[1], false);
    const children = [];
    if (xml.startsWith('/>', pos)) { pos += 2; return [name, attrs, children]; }
    if (xml[pos] !== '>') fail('tag end expected');
    pos++;
    for (;;) {
      if (xml.startsWith('</', pos)) {
        const close = /^<\/([^>\s]+)\s*>/.exec(xml.slice(pos)) ?? fail('bad close tag');
        if (close[1] !== head[1]) fail(`mismatched close ${close[1]}`);
        pos += close[0].length;
        return [name, attrs, children];
      }
      if (xml[pos] === '<') {
        if (xml.startsWith('<!', pos) || xml.startsWith('<?', pos)) fail('unsupported markup');
        children.push(element(scope));
      } else {
        const end = xml.indexOf('<', pos);
        if (end < 0) fail('unterminated');
        children.push(unescape(xml.slice(pos, end))); pos = end;
      }
    }
  }
  const tree = element({});
  if (pos !== xml.length) fail('trailing content');
  return tree;
}
