import type { ReactNode } from "react";
import { BrandMark } from "../../app/BrandMark";

/**
 * Shared frame of the sign-in steps: a card on the cream page whose top band carries the spa's
 * mark in gold on maroon, like the printed menu. `brand` is the band's text — the login page passes
 * its h1, later steps a plain wordmark so each screen keeps exactly one h1.
 */
export function AuthFrame({ brand, children }: { brand: ReactNode; children: ReactNode }) {
  return (
    <main className="flex min-h-dvh items-center justify-center p-4">
      <div className="w-full max-w-sm overflow-hidden rounded-[14px] border border-border bg-surface shadow-float">
        <div className="bg-chrome bg-[linear-gradient(160deg,var(--chrome-bg-2),var(--chrome-bg))] px-6 pt-6 pb-5 text-center text-chrome-text [--focus:var(--chrome-accent)]">
          <BrandMark className="mx-auto size-14 text-chrome-accent" />
          {brand}
          <p className="mt-1 text-xs tracking-[0.18em] text-chrome-text-muted uppercase">
            Hồi sinh sức khỏe và sắc đẹp
          </p>
        </div>
        <div aria-hidden="true" className="h-0.5 bg-[linear-gradient(90deg,transparent,var(--chrome-accent),transparent)]" />
        {children}
      </div>
    </main>
  );
}

/** Classes of the brand name inside the band (h1 on the login page, plain text elsewhere). */
export const bandTitle = "mt-2 font-brand text-2xl text-chrome-accent";
