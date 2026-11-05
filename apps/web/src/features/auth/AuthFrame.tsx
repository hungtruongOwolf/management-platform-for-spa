import type { ReactNode } from "react";
import { BrandEmblem } from "../../app/BrandMark";

/**
 * Shared frame of the sign-in steps. Desktop: the spa's real emblem in gold on a maroon panel
 * (with the paper books' corner ornaments) beside the form. Phones: the emblem heads the card.
 * `brand` is the panel's title — the login page passes its h1, later steps a plain wordmark, so
 * each screen keeps exactly one h1.
 */
export function AuthFrame({ brand, children }: { brand: ReactNode; children: ReactNode }) {
  return (
    <main className="grid min-h-dvh place-items-center p-4 sm:p-8">
      <div className="card grid w-full max-w-4xl overflow-hidden shadow-float md:grid-cols-[1.05fr_1fr]">
        <div className="chrome ornament relative flex flex-col items-center justify-center px-6 pt-8 pb-6 text-center [--ornament:var(--chrome-accent)] md:px-10 md:py-14">
          <BrandEmblem className="w-28 text-chrome-accent md:w-56" />
          {brand}
          <p className="mt-1 text-xs tracking-[0.18em] text-chrome-text-muted uppercase">Hồi sinh sức khoẻ và sắc đẹp</p>
        </div>
        <div className="flex flex-col justify-center">
          <div aria-hidden="true" className="h-0.5 bg-[linear-gradient(90deg,transparent,var(--chrome-accent),transparent)] md:hidden" />
          {children}
        </div>
      </div>
    </main>
  );
}

/** Classes of the brand name inside the panel (h1 on the login page, plain text elsewhere). */
export const bandTitle = "mt-4 font-brand text-2xl text-chrome-accent md:sr-only";
