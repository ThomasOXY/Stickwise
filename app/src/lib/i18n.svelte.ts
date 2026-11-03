import { invoke } from "@tauri-apps/api/core";
import common from "../locales/common.json";
import deck from "../locales/deck.json";
import exportOpts from "../locales/export.json";
import library from "../locales/library.json";
import peer from "../locales/peer.json";
import planner from "../locales/planner.json";
import player from "../locales/player.json";
import settings from "../locales/settings.json";
import shell from "../locales/shell.json";
import stick from "../locales/stick.json";
import sync from "../locales/sync.json";

/**
 * Translations. Each file in `src/locales` is one part of the app with all
 * languages side by side: `{ "cs": {…}, "en": {…}, "de": {…} }`. Czech is
 * the reference: a key missing in another language fails the type check
 * (and `npm run check`, which also compares the placeholders).
 *
 * A value is a string with `{name}` placeholders, or plural forms
 * `{ "one": …, "few": …, "other": … }` picked by `{count}` (Czech has
 * one/few/other, English and German one/other).
 */
const NAMESPACES = { common, deck, export: exportOpts, library, peer, planner, player, settings, shell, stick, sync };

export const LANGS = ["cs", "en", "de"] as const;
export type Lang = (typeof LANGS)[number];
/** What the DJ chose in the settings; `auto` follows macOS. */
export type LangChoice = Lang | "auto";

type NS = typeof NAMESPACES;
type Plural = { one: string; other: string; few?: string };
type Paths<T, P extends string = ""> = {
  [K in keyof T & string]: T[K] extends string | Plural ? `${P}${K}` : Paths<T[K], `${P}${K}.`>;
}[keyof T & string];
/** Every translation key, `namespace.path.to.text`. */
export type Key = Paths<{ [N in keyof NS]: NS[N]["cs"] }>;

// Every language has the keys Czech has (plural forms may differ).
type Shape<T> = T extends string ? "text" : T extends Plural ? "plural" : { [K in keyof T]: Shape<T[K]> };
type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;
type AllMatch = { [N in keyof NS]: Same<Shape<NS[N]["en"]>, Shape<NS[N]["cs"]>> & Same<Shape<NS[N]["de"]>, Shape<NS[N]["cs"]>> }[keyof NS];
const allMatch: AllMatch = true;
void allMatch;

type Tree = { [k: string]: string | Plural | Tree };
const dicts = Object.fromEntries(
  LANGS.map((l) => [l, Object.fromEntries(Object.entries(NAMESPACES).map(([n, d]) => [n, (d as Record<Lang, Tree>)[l]]))]),
) as Record<Lang, Tree>;

const rules = Object.fromEntries(LANGS.map((l) => [l, new Intl.PluralRules(l)])) as Record<Lang, Intl.PluralRules>;

function lookup(tree: Tree, key: string): string | Plural | undefined {
  let v: string | Plural | Tree | undefined = tree;
  for (const part of key.split(".")) {
    if (typeof v !== "object" || v === null) return undefined;
    v = (v as Tree)[part];
  }
  return v as string | Plural | undefined;
}

class I18n {
  /** The language in use. */
  lang = $state<Lang>("en");

  /** Pick the language: the DJ's choice, or the first of macOS's we have. */
  async apply(choice: LangChoice) {
    let lang: Lang = "en";
    if (choice !== "auto") {
      lang = choice;
    } else {
      const system = await invoke<string[]>("system_languages").catch(() => [] as string[]);
      const all = [...system, ...navigator.languages];
      lang = (all.map((l) => l.slice(0, 2).toLowerCase()).find((l) => (LANGS as readonly string[]).includes(l)) as Lang) ?? "en";
    }
    this.lang = lang;
    document.documentElement.lang = lang;
    await invoke("set_locale", { lang }).catch(() => {});
  }
}

export const i18n = new I18n();

/**
 * The text for `key` in the current language, `{name}` placeholders filled
 * from `params`; plural forms are picked by `params.count`.
 */
export function t(key: Key, params?: Record<string, string | number>): string {
  const lang = i18n.lang;
  let v = lookup(dicts[lang], key) ?? lookup(dicts.en, key);
  if (v === undefined) return key;
  if (typeof v === "object") {
    const form = rules[lang].select(Number(params?.count ?? 0)) as keyof Plural;
    v = v[form] ?? v.other;
  }
  return v.replace(/\{(\w+)\}/g, (m, k) => (params?.[k] !== undefined ? String(params[k]) : m));
}
