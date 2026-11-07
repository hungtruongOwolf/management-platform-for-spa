/**
 * Confirmation for a package bought for someone else ("Mua cho người khác", US13): the package
 * sits on the beneficiary's profile, the payer only paid, so the toast names both.
 * `Đã bán "Gói X" cho Nguyễn Văn A (Nguyễn Thị Hà trả tiền).`
 */
export function boughtForOtherText(
  packageName: string,
  beneficiaryName: string,
  payerName: string,
): string {
  return `Đã bán "${packageName}" cho ${beneficiaryName} (${payerName} trả tiền).`;
}
