# panel/src/i18n — where new copy goes

One file per key prefix (`chat.*` → `chat.ts`, `sessions.*` → `sessions.ts`, …), and
`panel/src/i18n.tsx` only assembles them into the dictionaries the provider serves.

## The rule

1. **New copy for a view goes in that view's domain file.** If you are adding a
   string for the chat view, it goes in `chat.ts`; for sessions, `sessions.ts`.
   That is the whole point of the split: two tasks editing two different views
   touch two different files, so they no longer serialise on one file.
2. **Add every key to BOTH `zh` and `en` in the same file.** Both dictionaries live
   side by side in the domain file, so a half-translated key is visible in review.
3. **Cross-view copy goes in `common.ts`.** A button or empty state several views
   share belongs there — that file is shared on purpose, so keep it small.
4. **`t("...")` call sites never change.** The domain dictionaries are spread back
   into the same shape in `i18n.tsx`; `useI18n().t(key, params)` is unchanged.
5. **Values are data, not documentation.** Do not record measurements, ratios or
   counts in copy or in comments here (see the t159 rule: thresholds and *why* are
   knowledge, measured readings are transcripts that go stale — point at the audit
   row instead).

## Adding a whole new domain

Create `panel/src/i18n/<domain>.ts` exporting `zh` and `en` (`Record<string, string>`),
then add one `import * as <domain>Domain from "./i18n/<domain>";` and one
`...<domain>Domain.zh,` / `...<domain>Domain.en,` line to `i18n.tsx`.

## Why the keys are not typed more tightly

`t(key)` takes a plain string and falls back to the key itself when a lookup misses
(`dicts[lang][key] ?? dicts.en[key] ?? key`), which keeps a missing key visible at
runtime instead of blanking the UI. The compile-time constraint that does exist is
`ROUTE_TITLE_KEY: Record<View["kind"], string>` in `App.tsx`: adding a route without
a title key is a type error. That constraint is untouched by this split.
