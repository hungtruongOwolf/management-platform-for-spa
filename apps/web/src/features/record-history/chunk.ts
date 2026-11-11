// The lazy chunk of record history (design record-history §1.1): every screen loads it through
// `lazy.tsx`, /quay prefetches it after the first paint for those who may see it.
export { RecordTrace } from "./RecordTrace";
export { LedgerSaleSheet } from "./LedgerSaleSheet";
