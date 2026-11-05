import type { ReactNode } from "react";
import { BrandLogo } from "../../app/BrandLogo";

/**
 * Shared frame of the sign-in steps. Desktop: the spa's whole logo in gold on a maroon panel (with
 * the paper books' corner ornaments) beside the form. Phones: the logo heads the card. The logo
 * already carries the name and the curved tagline, so neither is repeated as visible text (EM
 * 2026-11-05 round 3). `brand` is the panel's title for screen readers — the login page passes its
 * h1, later steps a plain paragraph, so each screen keeps exactly one h1.
 */
export function AuthFrame({ brand, children }: { brand: ReactNode; children: ReactNode }) {
  return (
    <main className="grid min-h-dvh place-items-center p-4 sm:p-8">
      <div className="card grid w-full max-w-4xl overflow-hidden shadow-float md:grid-cols-[1.05fr_1fr]">
        <div className="chrome ornament relative flex flex-col items-center justify-center px-6 pt-8 pb-6 text-center [--ornament:var(--chrome-accent)] md:px-10 md:py-14">
          <BrandLogo className="w-36 md:w-60" />
          {brand}
        </div>
        <div className="flex flex-col justify-center">
          <div aria-hidden="true" className="h-0.5 bg-[linear-gradient(90deg,transparent,var(--chrome-accent),transparent)] md:hidden" />
          {children}
        </div>
      </div>
    </main>
  );
}

/** Classes of the brand name inside the panel (h1 on the login page, plain text elsewhere): the logo shows it. */
export const bandTitle = "sr-only";
