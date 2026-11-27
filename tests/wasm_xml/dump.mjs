// Neutral dump shared with tests/xml_runtime.rs `dump`: every declared attribute in
// order, null for absent, empty lists as null except the ordered-content token list.
export function dump(bundle, id, value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return value;
  const model = bundle.models[id];
  const out = {};
  for (const a of model.attributes) {
    const target = a.reference ?? (a.type.kind === 'model' ? a.type.id : null);
    out[a.name] = member(bundle, target, value[a.name] ?? null);
  }
  for (const [k, v] of Object.entries(value)) if (!(k in out) && k !== '__lml_content') out[k] = v;
  if ('__lml_content' in value) out.__lml_content = value.__lml_content;
  return out;
}
function member(bundle, target, v) {
  if (Array.isArray(v)) return v.length === 0 ? null : v.map(x => member(bundle, target, x));
  if (v !== null && typeof v === 'object' && target) return dump(bundle, target, v);
  return v;
}
// Ruby oracle encodings with no Rust counterpart: instances carry a __lml_model marker, and an
// absent any_attribute hash is the tagged object {state: "absent"} where Rust has null.
export const withoutRubyMarker = v => Array.isArray(v) ? v.map(withoutRubyMarker)
  : v && typeof v === 'object'
    ? (Object.keys(v).length === 1 && v.state === 'absent' ? null
      : Object.fromEntries(Object.entries(v).filter(([k]) => k !== '__lml_model').map(([k, x]) => [k, withoutRubyMarker(x)])))
    : v;
