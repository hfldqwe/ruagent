# Web Interface Guidelines —— 审计用的外部标准（本地归档）

**为什么归档**：`web-design-guidelines` 技能是薄壳，它要求每次审查前**联网抓取**规则集 ✓ 而本机到 `raw.githubusercontent.com` **连接超时** ✗（`github.com` 与 `vercel.com` 可通 ✓）⇒ 规则集**不可靠地可得** ⇒ 归档到仓库，让审计**可复现、离线可用** ✓

**来源（provenance）**：
- `https://vercel.com/design/guidelines` —— 抓取于本次会话（content 27321 字符）
- `https://github.com/vercel-labs/web-interface-guidelines/blob/main/command.md` —— 备用入口（content 7720 字符）
- 上游仓库：`vercel-labs/web-interface-guidelines`

**用法**：本文件是**审计判据的来源** ✓ —— 每一条被采纳的判据必须写明**它来自哪一行**（本代规则：规则的出处必须指名它的实例 ✓）

---

## 原文（vercel.com/design/guidelines）

[Vercel Home](https://vercel.com/home)

# Web Interface Guidelines

Interfaces succeed because of hundreds of choices. This is a living, non-exhaustive list of those decisions. Most guidelines are framework-agnostic, some specific to React/Next.js. [Feedback is welcome](https://github.com/vercel-labs/web-interface-guidelines/tree/main).

## [Interactions](https://vercel.com/design/guidelines\#interactions)

- **[Keyboard works everywhere.](https://vercel.com/design/guidelines#keyboard-works-everywhere)** All flows are keyboard-operable & follow the [WAI-ARIA Authoring Patterns](https://www.w3.org/WAI/ARIA/apg/patterns/).
- **[Clear focus.](https://vercel.com/design/guidelines#clear-focus)** Every focusable element shows a visible, unobscured focus ring. Prefer `:focus-visible` over `:focus` to avoid distracting pointer users. Set `:focus-within` for grouped controls. Sticky headers, footers, banners, & overlays never cover the focused element.
- **[Manage focus.](https://vercel.com/design/guidelines#manage-focus)** Use focus traps, move & return focus according to the [WAI-ARIA Patterns](https://www.w3.org/WAI/ARIA/apg/patterns/).
- **[Match visual & hit targets.](https://vercel.com/design/guidelines#match-visual-hit-targets)** Exception: if the visual target is < 24px, expand its hit target to ≥ 24px. On mobile, the minimum size is 44px.
- **[Mobile input size.](https://vercel.com/design/guidelines#mobile-input-size)**`<input>` font size is ≥ 16px on mobile to prevent iOS Safari auto-zoom/pan on focus. Or set `<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1" />`.
- **[Respect zoom.](https://vercel.com/design/guidelines#respect-zoom)** Never disable browser zoom.
- **[Hydration-safe inputs.](https://vercel.com/design/guidelines#hydration-safe-inputs)** Inputs must not lose focus or value after hydration.
- **[Don’t block paste.](https://vercel.com/design/guidelines#dont-block-paste)** Never disable paste in `<input>` or `<textarea>`.
- **[Loading buttons.](https://vercel.com/design/guidelines#loading-buttons)** Show a loading indicator & keep the original label.
- **[Minimum loading-state duration.](https://vercel.com/design/guidelines#minimum-loading-state-duration)** If you show a spinner/skeleton, add a short show-delay (~150–300 ms) & a minimum visible time (~300–500 ms) to avoid flicker on fast responses. The `<Suspense>` component in React does this automatically.
- **[URL as state.](https://vercel.com/design/guidelines#url-as-state)** Persist state in the URL so share, refresh, Back/Forward navigation work e.g., [nuqs](https://nuqs.dev/).
- **[Optimistic updates.](https://vercel.com/design/guidelines#optimistic-updates)** Update the UI immediately when success is likely; reconcile on server response. On failure, show an error & roll back or provide Undo.
- **[Ellipsis for further input & loading states.](https://vercel.com/design/guidelines#ellipsis-for-further-input-loading-states)** Menu options that open a follow-up e.g., "Rename…" & loading/processing states e.g., "Loading…", "Saving…", "Generating…" end with an ellipsis.
- **[Confirm destructive actions.](https://vercel.com/design/guidelines#confirm-destructive-actions)** Require confirmation or provide Undo with a safe window.
- **[Prevent double-tap zoom on controls.](https://vercel.com/design/guidelines#prevent-double-tap-zoom-on-controls)** Set `touch-action: manipulation`.
- **[Tap highlight follows design.](https://vercel.com/design/guidelines#tap-highlight-follows-design)** Set `webkit-tap-highlight-color`.
- **[Design forgiving interactions.](https://vercel.com/design/guidelines#design-forgiving-interactions)** Controls minimize finickiness with generous hit targets, clear affordances, & predictable interactions, e.g., [prediction cones](https://x.com/JohnPhamous/status/1657083267299028992).
- **[Tooltip timing.](https://vercel.com/design/guidelines#tooltip-timing)** Delay the first tooltip in a group; [subsequent peers have no delay](https://x.com/emilkowalski_/status/1962500739336462340).
- **[Overscroll behavior.](https://vercel.com/design/guidelines#overscroll-behavior)** Set `overscroll-behavior: contain` intentionally e.g., in modals/drawers.
- **[Scroll positions persist.](https://vercel.com/design/guidelines#scroll-positions-persist)** Back/Forward restores prior scroll.
- **[Autofocus for speed.](https://vercel.com/design/guidelines#autofocus-for-speed)** On desktop screens with a single primary input, autofocus. Rarely autofocus on mobile because the keyboard opening can cause layout shift.
- **[No dead zones.](https://vercel.com/design/guidelines#no-dead-zones)** If part of a control looks interactive, it should be interactive. Don’t leave users guessing where to interact.
- **[Deep-link everything.](https://vercel.com/design/guidelines#deep-link-everything)** Filters, tabs, pagination, expanded panels, anytime `useState` is used.
- **[Clean drag interactions.](https://vercel.com/design/guidelines#clean-drag-interactions)** Disable text selection & apply [`inert`](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Global_attributes/inert) (which prevents interaction) while an element is dragged so selection/hover don't occur simultaneously.
- **[Gestures have alternatives.](https://vercel.com/design/guidelines#gestures-have-alternatives)** Every drag, swipe, pinch, or path-based action also works with a [tap/click control & keyboard](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html) unless the gesture is essential.
- **[Links are links.](https://vercel.com/design/guidelines#links-are-links)** Use `<a>` or `<Link>` for navigation so standard browser behaviors work (Cmd/Ctrl+Click, middle-click, right-click to open in a new tab). Never substitute with `<button>` or `<div>` for navigational links.
- **[Announce async updates.](https://vercel.com/design/guidelines#announce-async-updates)** Use polite aria-live for toasts & inline validation.
- **[Locale-aware keyboard shortcuts.](https://vercel.com/design/guidelines#locale-aware-keyboard-shortcuts)** Internationalize keyboard shortcuts for non-QWERTY layouts. Show platform-specific symbols.

## [Animations](https://vercel.com/design/guidelines\#animations)

- **[Honor `prefers-reduced-motion`.](https://vercel.com/design/guidelines#honor-prefers-reduced-motion)** Provide a reduced-motion variant.
- **[Implementation preference.](https://vercel.com/design/guidelines#implementation-preference)**Prefer CSS, avoid main-thread JS-driven animations when possible.
  - Preference: CSS > Web Animations API > JavaScript libraries e.g., [motion](https://www.npmjs.com/package/motion).
- **[Compositor-friendly.](https://vercel.com/design/guidelines#compositor-friendly)** Prioritize GPU-accelerated properties (`transform`, `opacity`) & avoid properties that trigger reflows/repaints (`width`, `height`, `top`, `left`).
- **[Necessity check.](https://vercel.com/design/guidelines#necessity-check)** Only animate when it clarifies cause & effect or when it adds deliberate delight e.g., [the northern lights](https://x.com/JohnPhamous/status/1831380516509278561).
- **[Easing fits the subject.](https://vercel.com/design/guidelines#easing-fits-the-subject)** Choose easing based on what changes (size, distance, trigger).
- **[Interruptible.](https://vercel.com/design/guidelines#interruptible)** Animations are cancelable by user input.
- **[Input-driven.](https://vercel.com/design/guidelines#input-driven)** Avoid autoplay except for muted, non-essential loops. Automatically playing motion lasting [>5 seconds](https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html) alongside other content has pause, stop, or hide controls.
- **[Correct transform origin.](https://vercel.com/design/guidelines#correct-transform-origin)** Anchor motion to where it “physically” starts.
- **[Never `transition: all`.](https://vercel.com/design/guidelines#never-transition-all)** Explicitly list only the properties you intend to animate (typically `opacity`, `transform`). `all` can unintentionally animate layout-affecting properties causing jank.
- **[Cross-browser SVG transforms.](https://vercel.com/design/guidelines#cross-browser-svg-transforms)** Apply CSS transforms/animations to `<g>` wrappers & set
`transform-box: fill-box; transform-origin: center;`. Safari historically had bugs with transform-origin on SVG & grouping avoids origin miscalculation.

## [Layout](https://vercel.com/design/guidelines\#layout)

- **[Optical alignment.](https://vercel.com/design/guidelines#optical-alignment)** [Adjust ±1px](https://x.com/JohnPhamous/status/1760444698857230360) when perception beats geometry.
- **[Deliberate alignment.](https://vercel.com/design/guidelines#deliberate-alignment)** Every element aligns with something intentionally whether to a grid, baseline, edge, or optical center. No accidental positioning.
- **[Balance contrast in lockups.](https://vercel.com/design/guidelines#balance-contrast-in-lockups)** When text & icons sit side by side, adjust weight, size, spacing, or color so they don’t clash. For example, a thin-stroke icon may need a bolder stroke next to medium-weight text.
- **[Responsive coverage.](https://vercel.com/design/guidelines#responsive-coverage)** Verify on mobile, laptop, & ultra-wide. For ultra-wide, zoom out to 50% to simulate.
- **[Respect safe areas.](https://vercel.com/design/guidelines#respect-safe-areas)** Account for notches & insets with [safe-area variables](https://developer.mozilla.org/en-US/docs/Web/CSS/env).
- **[No excessive scrollbars.](https://vercel.com/design/guidelines#no-excessive-scrollbars)** Only render useful scrollbars; fix overflow issues to prevent unwanted scrollbars. On macOS set ["Show scroll bars" to "Always"](https://support.apple.com/guide/mac-help/change-appearance-settings-mchlp1225/mac#:~:text=or%20status%20bars.-,Show%20scroll%20bars,-Scroll%20bars%20appear) to test what Windows users would see.
- **[Let the browser size things.](https://vercel.com/design/guidelines#let-the-browser-size-things)** Prefer flex/grid/intrinsic layout over measuring in JS. Avoid layout thrash by letting CSS handle flow, wrapping, & alignment.

## [Content](https://vercel.com/design/guidelines\#content)

- **[Inline help first.](https://vercel.com/design/guidelines#inline-help-first)** Prefer inline explanations; use tooltips as a last resort.
- **[Stable skeletons.](https://vercel.com/design/guidelines#stable-skeletons)** Skeletons mirror final content exactly to avoid layout shift.
- **[Accurate page titles.](https://vercel.com/design/guidelines#accurate-page-titles)**`<title>` reflects the current context.
- **[No dead ends.](https://vercel.com/design/guidelines#no-dead-ends)** Every screen offers a next step or recovery path.
- **[All states designed.](https://vercel.com/design/guidelines#all-states-designed)** Empty, sparse, dense, & error states.
- **[Typographic quotes.](https://vercel.com/design/guidelines#typographic-quotes)** Prefer curly quotes (“ ”) over straight quotes (" ").
- **[Avoid widows/orphans.](https://vercel.com/design/guidelines#avoid-widowsorphans)** Tidy rag & line breaks.
- **[Tabular numbers for comparisons.](https://vercel.com/design/guidelines#tabular-numbers-for-comparisons)** Use `font-variant-numeric: tabular-nums` or a monospace like [Geist Mono](https://vercel.com/font).
- **[Redundant status cues.](https://vercel.com/design/guidelines#redundant-status-cues)** Don’t rely on color alone; include text labels.
- **[Icons have labels.](https://vercel.com/design/guidelines#icons-have-labels)** Convey the same meaning with text for non-sighted users.
- **[Don’t ship the schema.](https://vercel.com/design/guidelines#dont-ship-the-schema)** Visual layouts may omit visible labels, but accessible names/labels still exist for assistive tech.
- **[Use the ellipsis character.](https://vercel.com/design/guidelines#use-the-ellipsis-character)**`…` over three periods `...`.
- **[Anchored headings.](https://vercel.com/design/guidelines#anchored-headings)** Set `scroll-margin-top` for headers when linking to sections.
- **[Resilient to user-generated content.](https://vercel.com/design/guidelines#resilient-to-user-generated-content)** Layouts handle short, average, & very long content.
- **[Locale-aware formats.](https://vercel.com/design/guidelines#locale-aware-formats)** Format dates, times, numbers, delimiters, & currencies for the user’s locale.
- **[Prefer language settings over location.](https://vercel.com/design/guidelines#prefer-language-settings-over-location)** Detect language via `Accept-Language` header & `navigator.languages`. Never rely on IP/GPS for language.
- **[Shield verbatim content from translation.](https://vercel.com/design/guidelines#shield-verbatim-content-from-translation)** Wrap brand names, product names, code tokens, & technical identifiers with `translate="no"` so browser auto-translate leaves them intact.
- **[Accessible content.](https://vercel.com/design/guidelines#accessible-content)** Set accurate names (`aria-label`), hide decoration (`aria-hidden`) & verify in the [accessibility tree](https://developer.chrome.com/blog/full-accessibility-tree).
- **[Icon-only buttons are named.](https://vercel.com/design/guidelines#icon-only-buttons-are-named)** Provide a descriptive `aria-label`.
- **[Semantics before ARIA.](https://vercel.com/design/guidelines#semantics-before-aria)** Prefer native elements (`button`, `a`, `label`, `table`), before `aria-*`.
- **[Headings & skip link.](https://vercel.com/design/guidelines#headings-skip-link)** Hierarchical `<h1–h6>` & a “Skip to content” link.
- **[Accessible media.](https://vercel.com/design/guidelines#accessible-media)** [Caption speech & meaningful sounds](https://www.w3.org/WAI/media/av/), provide transcripts for audio-only content, & describe essential visual information. Decorative media is hidden from assistive tech; media controls are keyboard-operable.
- **[Brand resources from the logo.](https://vercel.com/design/guidelines#brand-resources-from-the-logo)** [Right-click the nav logo](https://x.com/JohnPhamous/status/1636427186566762496) to surface brand assets for quick access.
- **[Non-breaking spaces for glued terms.](https://vercel.com/design/guidelines#non-breaking-spaces-for-glued-terms)** Use a non-breaking space `&nbsp;` to keep units, shortcuts & names together: `10 MB` → `10&nbsp;MB`, `⌘ + K` → `⌘&nbsp;+&nbsp;K`, `Vercel SDK` → `Vercel&nbsp;SDK`. Use `&#x2060;` for no space.

## [Forms](https://vercel.com/design/guidelines\#forms)

- **[Enter submits.](https://vercel.com/design/guidelines#enter-submits)** When a text input is focused, Enter submits if it's the only control. If there are many controls, apply to the last control.
- **[Textarea behavior.](https://vercel.com/design/guidelines#textarea-behavior)** In `<textarea>`, ⌘/⌃+Enter submits; Enter inserts a new line.
- **[Labels everywhere.](https://vercel.com/design/guidelines#labels-everywhere)** Every control has a `<label>` or is associated with a label for assistive tech.
- **[Label activation.](https://vercel.com/design/guidelines#label-activation)** Clicking a `<label>` focuses the associated control.
- **[Submission rule.](https://vercel.com/design/guidelines#submission-rule)** Keep submit enabled until submission starts; then disable during the in-flight request, show a spinner, & include an idempotency key.
- **[Don’t block typing.](https://vercel.com/design/guidelines#dont-block-typing)** Even if a field only accepts numbers, allow any input & show validation feedback. Blocking keystrokes entirely is confusing because the user gets no explanation.
- **[Don’t pre-disable submit.](https://vercel.com/design/guidelines#dont-pre-disable-submit)** Allow submitting incomplete forms to surface validation feedback.
- **[No dead zones on controls.](https://vercel.com/design/guidelines#no-dead-zones-on-controls)** Checkboxes & radios avoid dead zones; the label & control share a single generous hit target.
- **[Error placement.](https://vercel.com/design/guidelines#error-placement)** Show errors next to their fields; on submit, focus the first error.
- **[Autocomplete & names.](https://vercel.com/design/guidelines#autocomplete-names)** Set `autocomplete` & meaningful `name` values to enable autofill.
- **[Spellcheck selectively.](https://vercel.com/design/guidelines#spellcheck-selectively)** Disable for emails, codes, usernames, etc.
- **[Correct types & input modes.](https://vercel.com/design/guidelines#correct-types-input-modes)** Use the right `type` & `inputmode` for better keyboards & validation.
- **[Placeholders signal emptiness.](https://vercel.com/design/guidelines#placeholders-signal-emptiness)** End with an ellipsis.
- **[Placeholder value.](https://vercel.com/design/guidelines#placeholder-value)** Set placeholder to an example value or pattern e.g., `+1 (123) 456-7890` & `sk-012345679…`
- **[Unsaved changes.](https://vercel.com/design/guidelines#unsaved-changes)** Warn before navigation when data could be lost.
- **[Password managers & 2FA.](https://vercel.com/design/guidelines#password-managers-2fa)** Ensure compatibility & allow pasting one-time codes.
- **[Don’t trigger password managers for non-auth fields.](https://vercel.com/design/guidelines#dont-trigger-password-managers-for-non-auth-fields)** For inputs like “Search” avoid reserved names (e.g., password), use `autocomplete="off"` or a specific token like `autocomplete="one-time-code"` for OTP fields.
- **[Text replacements & expansions.](https://vercel.com/design/guidelines#text-replacements-expansions)** Some input methods add trailing whitespace. The input should trim the value to avoid showing a confusing error message.
- **[Windows `<select>` background.](https://vercel.com/design/guidelines#windows-select-background)** Explicitly set `background-color` & `color` on native `<select>` to avoid dark-mode contrast bugs on Windows.

## [Performance](https://vercel.com/design/guidelines\#performance)

- **[Device/browser matrix.](https://vercel.com/design/guidelines#devicebrowser-matrix)** Test iOS Low Power Mode & macOS Safari.
- **[Measure reliably.](https://vercel.com/design/guidelines#measure-reliably)** Disable extensions that add overhead or change runtime behavior.
- **[Track re-renders.](https://vercel.com/design/guidelines#track-re-renders)** Minimize & make re-renders fast. Use [React DevTools](https://react.dev/learn/react-developer-tools) or [React Scan](https://react-scan.com/).
- **[Throttle when profiling.](https://vercel.com/design/guidelines#throttle-when-profiling)** Test with CPU & network throttling.
- **[Minimize layout work.](https://vercel.com/design/guidelines#minimize-layout-work)** Batch reads/writes; avoid unnecessary reflows/repaints.
- **[Network latency budgets.](https://vercel.com/design/guidelines#network-latency-budgets)**`POST/PATCH/DELETE` complete in <500ms.
- **[Keystroke cost.](https://vercel.com/design/guidelines#keystroke-cost)** Prefer uncontrolled inputs; make controlled loops cheap.
- **[Large lists.](https://vercel.com/design/guidelines#large-lists)** Virtualize large lists e.g., [virtua](https://github.com/inokawa/virtua) or [content-visibility: auto](https://web.dev/articles/content-visibility).
- **[Preload wisely.](https://vercel.com/design/guidelines#preload-wisely)** Preload only above-the-fold images; lazy-load the rest.
- **[No image-caused CLS.](https://vercel.com/design/guidelines#no-image-caused-cls)** Set explicit image dimensions & reserve space.
- **[Preconnect to origins.](https://vercel.com/design/guidelines#preconnect-to-origins)** Use `<link rel="preconnect">` for asset/CDN domains (with crossorigin when needed) to reduce DNS/TLS latency.
- **[Preload fonts.](https://vercel.com/design/guidelines#preload-fonts)** For critical text to avoid flash & layout shift.
- **[Subset fonts.](https://vercel.com/design/guidelines#subset-fonts)** Ship only the code points/scripts you use via unicode-range (limit variable axes to what you need) to shrink size.
- **[Don’t use the main thread for expensive work.](https://vercel.com/design/guidelines#dont-use-the-main-thread-for-expensive-work)** Move especially long tasks to [Web Workers](https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API) to avoid blocking interaction with the page.
- **[Video over animated GIF.](https://vercel.com/design/guidelines#video-over-animated-gif)** Prefer `<video autoplay muted loop playsinline>` for looping animation. [Modern video is typically much smaller than GIF](https://web.dev/articles/replace-gifs-with-videos). Provide a still alternative & honor `prefers-reduced-motion`.
- **[Safari video-as-image.](https://vercel.com/design/guidelines#safari-video-as-image)** For short, non-essential loops, add an [H.264 MP4 source to `<picture>`](https://developer.apple.com/documentation/webkit/delivering-video-content-for-safari#Use-MP4-Video-Instead-of-Animated-GIFs). Safari renders the video through its image pipeline; other browsers use the still fallback. Gate the MP4 source with `prefers-reduced-motion`. Use `<video>` when users need playback controls or the animation conveys essential information.

## [Design](https://vercel.com/design/guidelines\#design)

- **[Layered shadows.](https://vercel.com/design/guidelines#layered-shadows)** Mimic ambient + direct light with at least two layers.
- **[Crisp borders.](https://vercel.com/design/guidelines#crisp-borders)** Combine borders & shadows; semi-transparent borders improve edge clarity.
- **[Nested radii.](https://vercel.com/design/guidelines#nested-radii)** Child radius ≤ parent radius & concentric so curves align.
- **[Hue consistency.](https://vercel.com/design/guidelines#hue-consistency)** On non-neutral backgrounds, tint borders/shadows/text toward the same hue.
- **[Accessible charts.](https://vercel.com/design/guidelines#accessible-charts)** Use color-blind-friendly palettes.
- **[Minimum contrast.](https://vercel.com/design/guidelines#minimum-contrast)** Prefer [APCA](https://apcacontrast.com/) over [WCAG 2](https://webaim.org/resources/contrastchecker/) for more accurate perceptual contrast.
- **[Interactions increase contrast.](https://vercel.com/design/guidelines#interactions-increase-contrast)**`:hover`, `:active`, `:focus` have more contrast than rest state.
- **[Browser UI matches your background.](https://vercel.com/design/guidelines#browser-ui-matches-your-background)** Set `<meta name="theme-color" content="#000000">` to [align the browser’s theme color with the page background](https://x.com/JohnPhamous/status/1816160187839107342).
- **[Set the appropriate color-scheme.](https://vercel.com/design/guidelines#set-the-appropriate-color-scheme)** Style the `<html>` tag with `color-scheme: dark` in dark themes so that scrollbars and other device UI have proper contrast.
- **[Text anti-aliasing & transforms.](https://vercel.com/design/guidelines#text-anti-aliasing-transforms)** Scaling text can change smoothing. Prefer animating a wrapper instead of the text node. If artifacts persist set `translateZ(0)` or `will-change: transform` to promote to its own layer.
- **[Avoid gradient banding.](https://vercel.com/design/guidelines#avoid-gradient-banding)** Fading content to dark colors using css masks can cause banding. [Background images can be used instead](https://x.com/JohnPhamous/status/1724491202148675590).

# [Vercel-specific](https://vercel.com/design/guidelines\#vercel-specific)

These preferences reflect Vercel’s brand & product choices. They aren’t universal guidelines.

## [Copywriting](https://vercel.com/design/guidelines\#copywriting)

- **[Active voice.](https://vercel.com/design/guidelines#active-voice)**
  - Instead of “ _The CLI will be installed,”_ say _“Install the CLI.”_
- **[Headings & buttons use Title Case](https://vercel.com/design/guidelines#headings-buttons-use-title-case)** ( [Chicago](https://title.sh/)). On marketing pages, use sentence case.
- **[Be clear & concise.](https://vercel.com/design/guidelines#be-clear-concise)** Use as few words as possible.
- **[Prefer `&` over `and`.](https://vercel.com/design/guidelines#prefer-over-and)**
- **[Action-oriented language.](https://vercel.com/design/guidelines#action-oriented-language)**
  - Instead of _“You will need the CLI…”_ say _“Install the CLI…”_.
- **[Keep nouns consistent.](https://vercel.com/design/guidelines#keep-nouns-consistent)** Introduce as few unique terms as possible.
- **[Write in second person.](https://vercel.com/design/guidelines#write-in-second-person)** Avoid first person.
- **[Use consistent placeholders.](https://vercel.com/design/guidelines#use-consistent-placeholders)**
  - Strings: `YOUR_API_TOKEN_HERE`. Numbers: `0123456789`.
- **[Use numerals for counts.](https://vercel.com/design/guidelines#use-numerals-for-counts)**
  - Instead of _“eight deployments”_ say _“8 deployments”_.
- **[Consistent currency formatting.](https://vercel.com/design/guidelines#consistent-currency-formatting)** In any given context, display currency with either 0 or 2 decimal places, never mix both.
- **[Separate numbers & units with a space.](https://vercel.com/design/guidelines#separate-numbers-units-with-a-space)**
  - Instead of `10MB` say `10 MB`.
  - Use a non-breaking space e.g., `10&nbsp;MB`.
- **[Default to positive language.](https://vercel.com/design/guidelines#default-to-positive-language)**Frame messages in an encouraging, problem-solving way, even for errors.
  - Instead of _“Your deployment failed,”_ say _“Something went wrong—try again or contact support.”_
- **[Error messages guide the exit.](https://vercel.com/design/guidelines#error-messages-guide-the-exit)**Don’t just state what went wrong—tell the user how to fix it.
  - Instead of _“Invalid API key,”_ say _“Your API key is incorrect or expired. Generate a new key in your account settings.”_ The copy & buttons/links should educate & give a clear action.
- **[Avoid ambiguity.](https://vercel.com/design/guidelines#avoid-ambiguity)**Labels are clear & specific.
  - Instead of the button label _“Continue”_ say _“Save API Key”_.

# [Integrate with Agents](https://vercel.com/design/guidelines\#integrate-with-agents)

Use these guidelines with AI coding agents. Audit all generated interfaces.

## [Review Command](https://vercel.com/design/guidelines\#review-command)

Install the `web-design-guidelines` skill to review UI code:

```
npx skills add https://github.com/vercel-labs/agent-skills --skill web-design-guidelines
```

## [AGENTS.md](https://vercel.com/design/guidelines\#agentsmd)

Add [AGENTS.md](https://agents.md/) to your project so agents apply these guidelines during generation.

- [Download AGENTS.md](https://raw.githubusercontent.com/vercel-labs/web-interface-guidelines/main/AGENTS.md)

# [Join Vercel](https://vercel.com/design/guidelines\#join-vercel)

We’re hiring people who live for these details. [Check out the job postings](https://vercel.com/careers?function=Design).

* * *

Thanks to [Adam](https://x.com/argyleink), [Jimmy](https://x.com/wwwjim), [Jonnie](https://destroytoday.com/), [Lochie](https://x.com/lochieaxon), [Paco](https://pa.co/), [Joe](https://joebell.studio/), & [Austin](https://x.com/austin_malerba) for feedback.

---

## 备用入口原文（github.com blob）

---
description: Review UI code for Vercel Web Interface Guidelines compliance
argument-hint: <file-or-pattern>
---

# Web Interface Guidelines

Review these files for compliance: $ARGUMENTS

Read files, check against rules below. Output concise but comprehensive—sacrifice grammar for brevity. High signal-to-noise.

## Rules

### Accessibility

- Icon-only buttons need `aria-label`
- Form controls need `<label>` or `aria-label`
- Interactive elements need keyboard handlers (`onKeyDown`/`onKeyUp`)
- `<button>` for actions, `<a>`/`<Link>` for navigation (not `<div onClick>`)
- Images need `alt` (or `alt=""` if decorative)
- Decorative icons need `aria-hidden="true"`
- Async updates (toasts, validation) need `aria-live="polite"`
- Use semantic HTML (`<button>`, `<a>`, `<label>`, `<table>`) before ARIA
- Headings hierarchical `<h1>`–`<h6>`; include skip link for main content
- `scroll-margin-top` on heading anchors
- Meaningful media needs captions, transcripts, or descriptions as applicable
- Media controls need keyboard support; decorative media needs assistive-tech hiding

### Focus States

- Interactive elements need visible focus: `focus-visible:ring-*` or equivalent
- Never `outline-none` / `outline: none` without focus replacement
- Use `:focus-visible` over `:focus` (avoid focus ring on click)
- Group focus with `:focus-within` for compound controls
- Sticky headers/footers/overlays must not cover the focused element

### Forms

- Inputs need `autocomplete` and meaningful `name`
- Use correct `type` (`email`, `tel`, `url`, `number`) and `inputmode`
- Never block paste (`onPaste` + `preventDefault`)
- Labels clickable (`htmlFor` or wrapping control)
- Disable spellcheck on emails, codes, usernames (`spellCheck={false}`)
- Checkboxes/radios: label + control share single hit target (no dead zones)
- Submit button stays enabled until request starts; spinner during request
- Errors inline next to fields; focus first error on submit
- Placeholders end with `…` and show example pattern
- `autocomplete="off"` on non-auth fields to avoid password manager triggers
- Warn before navigation with unsaved changes (`beforeunload` or router guard)

### Animation

- Honor `prefers-reduced-motion` (provide reduced variant or disable)
- Animate `transform`/`opacity` only (compositor-friendly)
- Never `transition: all`—list properties explicitly
- Set correct `transform-origin`
- SVG: transforms on `<g>` wrapper with `transform-box: fill-box; transform-origin: center`
- Animations interruptible—respond to user input mid-animation
- Autoplay motion >5 seconds alongside other content needs pause, stop, or hide controls
- Muted decorative loops must stop under `prefers-reduced-motion`

### Typography

- `…` not `...`
- Curly quotes `“` `”` not straight `"`
- Non-breaking spaces: `10&nbsp;MB`, `⌘&nbsp;K`, brand names
- Loading states end with `…`: `"Loading…"`, `"Saving…"`
- `font-variant-numeric: tabular-nums` for number columns/comparisons
- Use `text-wrap: balance` or `text-pretty` on headings (prevents widows)

### Content Handling

- Text containers handle long content: `truncate`, `line-clamp-*`, or `break-words`
- Flex children need `min-w-0` to allow text truncation
- Handle empty states—don't render broken UI for empty strings/arrays
- User-generated content: anticipate short, average, and very long inputs

### Images

- `<img>` needs explicit `width` and `height` (prevents CLS)
- Below-fold images: `loading="lazy"`
- Above-fold critical images: `priority` or `fetchpriority="high"`

### Performance

- Large lists (>50 items): virtualize (`virtua`, `content-visibility: auto`)
- No layout reads in render (`getBoundingClientRect`, `offsetHeight`, `offsetWidth`, `scrollTop`)
- Batch DOM reads/writes; avoid interleaving
- Prefer uncontrolled inputs; controlled inputs must be cheap per keystroke
- Add `<link rel="preconnect">` for CDN/asset domains
- Critical fonts: `<link rel="preload" as="font">` with `font-display: swap`
- Prefer `<video autoplay muted loop playsinline>` over animated GIF; provide a still alternative
- Short non-essential loops: Safari H.264 MP4 `<picture>` source, `prefers-reduced-motion` media condition, and still fallback

### Navigation & State

- URL reflects state—filters, tabs, pagination, expanded panels in query params
- Links use `<a>`/`<Link>` (Cmd/Ctrl+click, middle-click support)
- Deep-link all stateful UI (if uses `useState`, consider URL sync via nuqs or similar)
- Destructive actions need confirmation modal or undo window—never immediate

### Touch & Interaction

- `touch-action: manipulation` (prevents double-tap zoom delay)
- `-webkit-tap-highlight-color` set intentionally
- `overscroll-behavior: contain` in modals/drawers/sheets
- During drag: disable text selection, `inert` on dragged elements
- Drag/swipe/pinch/path gestures need tap/click and keyboard alternatives unless essential
- `autoFocus` sparingly—desktop only, single primary input; avoid on mobile

### Safe Areas & Layout

- Full-bleed layouts need `env(safe-area-inset-*)` for notches
- Avoid unwanted scrollbars: `overflow-x-hidden` on containers, fix content overflow
- Flex/grid over JS measurement for layout

### Dark Mode & Theming

- `color-scheme: dark` on `<html>` for dark themes (fixes scrollbar, inputs)
- `<meta name="theme-color">` matches page background
- Native `<select>`: explicit `background-color` and `color` (Windows dark mode)

### Locale & i18n

- Dates/times: use `Intl.DateTimeFormat` not hardcoded formats
- Numbers/currency: use `Intl.NumberFormat` not hardcoded formats
- Detect language via `Accept-Language` / `navigator.languages`, not IP
- Brand names, code tokens, identifiers: wrap with `translate="no"` to prevent garbled auto-translation

### Hydration Safety

- Inputs with `value` need `onChange` (or use `defaultValue` for uncontrolled)
- Date/time rendering: guard against hydration mismatch (server vs client)
- `suppressHydrationWarning` only where truly needed

### Hover & Interactive States

- Buttons/links need `hover:` state (visual feedback)
- Interactive states increase contrast: hover/active/focus more prominent than rest

### Content & Copy

- Active voice: "Install the CLI" not "The CLI will be installed"
- Title Case for headings/buttons (Chicago style)
- Numerals for counts: "8 deployments" not "eight"
- Specific button labels: "Save API Key" not "Continue"
- Error messages include fix/next step, not just problem
- Second person; avoid first person
- `&` over "and" where space-constrained

### Anti-patterns (flag these)

- `user-scalable=no` or `maximum-scale=1` disabling zoom
- `onPaste` with `preventDefault`
- `transition: all`
- `outline-none` without focus-visible replacement
- Inline `onClick` navigation without `<a>`
- `<div>` or `<span>` with click handlers (should be `<button>`)
- Images without dimensions
- Large arrays `.map()` without virtualization
- Form inputs without labels
- Icon buttons without `aria-label`
- Hardcoded date/number formats (use `Intl.*`)
- `autoFocus` without clear justification
- Animated GIF when compressed video is suitable
- Gesture-only action without tap/click and keyboard alternative

## Output Format

Group by file. Use `file:line` format (VS Code clickable). Terse findings.

```text
## src/Button.tsx

src/Button.tsx:42 - icon button missing aria-label
src/Button.tsx:18 - input lacks label
src/Button.tsx:55 - animation missing prefers-reduced-motion
src/Button.tsx:67 - transition: all → list properties

## src/Modal.tsx

src/Modal.tsx:12 - missing overscroll-behavior: contain
src/Modal.tsx:34 - "..." → "…"

## src/Card.tsx

✓ pass
```

State issue + location. Skip explanation unless fix non-obvious. No preamble.
