/**
 * Shapes of the lucky draw API (architecture/lucky-draw.md §3), from the generated contract
 * (`@vpyd/api-client`, `pnpm api:gen`): a backend change that breaks them fails the typecheck. The
 * screen's own short names stand for the generated ones.
 */
export type {
  LuckyDrawDetail,
  LuckyDrawList,
  LuckyDrawSummary,
  LuckyDrawEligible as Eligible,
  LuckyDrawGender as Gender,
  LuckyDrawParticipant as Participant,
  LuckyDrawParticipantWrite as ParticipantWrite,
  LuckyDrawPrize as Prize,
  LuckyDrawPrizeWrite as PrizeWrite,
  LuckyDrawResult as DrawResult,
  LuckyDrawStatus as DrawStatus,
  SpinOutcome,
} from "@vpyd/api-client";
