/** Gender labels as the spa writes them (design-system.md "Định dạng"). */
const GENDER: Record<string, string> = { female: "Nữ", male: "Nam" };

export interface CustomerNameProps {
  name: string;
  /** `female` / `male` from the API; null/undefined/unknown values show no marker. */
  gender?: string | null | undefined;
  /** Show "Chưa rõ" when the gender is not recorded (profile header); lists stay quiet. */
  showUnknown?: boolean;
  className?: string;
}

/**
 * A customer's name with a small gender marker right beside it (EM 2026-11-05: "giới tính để bên
 * cạnh tên cho dễ phân biệt"), so two customers with the same name tell apart at a glance and the
 * desk picks the right price column. The marker is text ("Nữ"/"Nam"), never colour alone, and reads
 * as "Nguyễn Thị Hà, Nữ" to a screen reader. Inline: drop it inside a link, heading or table cell.
 */
export function CustomerName({ name, gender, showUnknown = false, className = "" }: CustomerNameProps) {
  const label = gender ? GENDER[gender] : undefined;
  return (
    <span className={`customer-name ${className}`}>
      <span className="customer-name-text">{name}</span>
      {label ? (
        <span className="gender-chip" data-gender={gender}>
          <span className="sr-only">,</span> {label}
        </span>
      ) : (
        showUnknown && (
          <span className="gender-chip" data-gender="unknown">
            <span className="sr-only">, giới tính</span> Chưa rõ
          </span>
        )
      )}
    </span>
  );
}
