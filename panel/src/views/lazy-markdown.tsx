// Lazy markdown renderer (chunk name: lazy-markdown-*, so the vendor
//
// chunk that carries the stack stays the only markdown-* one).
//
// ui.tsx ships a `Markdown` that imports react-markdown + remark-gfm at
// module scope. Because every view imports ui.tsx, that put the whole
// unified/micromark stack (~229KB, 16.3% of the bundle) in the ENTRY chunk
// even on routes that render no markdown at all.
//
// Nothing under views/ imports ui.tsx's `Markdown` any more, so Rollup
// tree-shakes it (react-markdown and remark-gfm both declare
// "sideEffects": false) and the stack leaves the entry. This component is
// the replacement: the renderer arrives on demand, so only the views that
// really show markdown pay for it.
//
// The shared-layer half of the fix (ui.tsx itself) is out of this task's
// scope; it is reported to the captain so systems can drop its copy.

import { Suspense, lazy } from "react";

const Renderer = lazy(async () => {
  const [{ default: ReactMarkdown }, { default: remarkGfm }] = await Promise.all([
    import("react-markdown"),
    import("remark-gfm"),
  ]);
  return {
    default: ({ children }: { children: string }) => (
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{children}</ReactMarkdown>
    ),
  };
});

/** Same contract as ui.tsx's Markdown: `{ children: string }` inside .md.
 *  Until the chunk lands it shows the source text, so a reader never sees
 *  an empty block (and a test asserting the text does not have to wait for
 *  the renderer). */
export function Markdown({ children }: { children: string }) {
  return (
    <div className="md">
      <Suspense fallback={<pre className="raw">{children}</pre>}>
        <Renderer>{children}</Renderer>
      </Suspense>
    </div>
  );
}
