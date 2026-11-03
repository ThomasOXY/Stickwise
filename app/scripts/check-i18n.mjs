// Checks the translations: every language has the keys Czech has, the same
// {placeholders}, and plural forms where Czech has them.
//
//   node scripts/check-i18n.mjs
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const DIRS = [new URL("../src/locales", import.meta.url).pathname, new URL("../../crates/i18n/locales", import.meta.url).pathname];
const LANGS = ["cs", "en", "de"];
const PLURAL = { cs: ["one", "few", "other"], en: ["one", "other"], de: ["one", "other"] };

const isPlural = (v) => v && typeof v === "object" && "other" in v;
const holes = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");

const problems = [];
function walk(file, lang, ref, other, path) {
  for (const [k, v] of Object.entries(ref)) {
    const p = path ? `${path}.${k}` : k;
    const o = other?.[k];
    if (o === undefined) {
      problems.push(`${file} ${lang}: missing ${p}`);
    } else if (typeof v === "string" || isPlural(v)) {
      const texts = (x) => (typeof x === "string" ? [x] : Object.values(x));
      if (isPlural(v) !== isPlural(o)) problems.push(`${file} ${lang}: ${p} should ${isPlural(v) ? "" : "not "}be plural`);
      else if (isPlural(o) && PLURAL[lang].some((f) => !(f in o))) problems.push(`${file} ${lang}: ${p} needs forms ${PLURAL[lang]}`);
      const want = new Set(texts(v).map(holes));
      for (const t of texts(o)) if (!want.has(holes(t))) problems.push(`${file} ${lang}: ${p} placeholders {${holes(t)}} ≠ Czech`);
    } else {
      walk(file, lang, v, o, p);
    }
  }
  for (const k of Object.keys(other ?? {})) if (!(k in ref)) problems.push(`${file} ${lang}: extra ${path ? `${path}.` : ""}${k}`);
}

let files = 0;
for (const dir of DIRS) {
  let names = [];
  try {
    names = readdirSync(dir).filter((n) => n.endsWith(".json"));
  } catch {
    continue;
  }
  for (const name of names) {
    files++;
    const data = JSON.parse(readFileSync(join(dir, name), "utf8"));
    for (const lang of LANGS) {
      if (!data[lang]) problems.push(`${name}: no ${lang}`);
      else if (lang !== "cs") walk(name, lang, data.cs, data[lang], "");
    }
  }
}

if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log(`translations ok (${files} files)`);
