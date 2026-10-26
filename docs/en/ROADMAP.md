# Roadmap

[Português](../pt-BR/ROADMAP.md) · **English**

What is planned, with no dates and in no particular order. An item here is not a promise: if
you want to help with one, open an issue first to agree on the approach
([CONTRIBUTING](CONTRIBUTING.md)).

## Interface in more languages

Today the interface is in Portuguese (Brazil), English and Spanish. To reach most languages:

- **Data-driven languages.** Discover the languages from the `i18n/` folders, instead of one
  variant in the code per language, and pick the system's with a fallback chain
  (`es-MX` → `es` → `en`), using `fluent-langneg`. Adding a language becomes just translating
  the `.ftl` files.
- **Fonts for other scripts** (in [Tomni UI](https://github.com/TirechCorp/tomni-ui)). Selawik
  covers only the Latin alphabet; Chinese, Japanese and Korean already exist through the `cjk`
  feature (about 4.5 MB). Arabic, Hebrew, Devanagari, Thai and others need bundled Noto fonts
  or the system fonts.
- **Right-to-left scripts** (in Tomni UI). Mirror the widget layout for Arabic and Hebrew;
  parley already handles the text order (bidi).
- **Number, date and size formats** through ICU4X, replacing the rules in `formatos.ftl`.
  Fluent already handles plurals.
- **Text that grows** (German, for example): check each new language with the clipped-text
  lint and the snapshots.
- **Search and line breaking** in languages without spaces between words. IME input and
  double-width characters in the terminal already exist.
- **Translation workflow:** a platform that reads `.ftl` (Weblate, Pontoon or Crowdin), with
  the pseudo-localization tests that already exist.
- **Library strings:** ready-made sets of Tomni UI's `Strings` per language, instead of the app
  filling in each text.

Text adds little to the binary (about 250 KB per language); fonts are what weigh.
